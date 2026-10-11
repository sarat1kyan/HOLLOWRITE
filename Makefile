# Developer entry points. Every target is a thin wrapper so the underlying
# command is easy to run by hand; see README.md for the walkthrough.

SHELL := /bin/bash
-include .env
export

SERVER_URL ?= http://127.0.0.1:7700
CARGO ?= cargo
NPM ?= npm

.PHONY: help check-env db-up db-down migrate seed dev dev-server dev-web dev-terminal web-install \
        build release test test-rust test-db test-web lint fmt smoke load latency capture clean

help:
	@grep -E '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-14s %s\n", $$1, $$2}'

check-env: ## Verify toolchain, database and ports
	@scripts/check-env.sh

db-up: ## Start PostgreSQL (Docker if available, else a local cluster)
	@scripts/db-up.sh

db-down: ## Stop the Docker PostgreSQL container
	docker compose stop db

migrate: ## Apply database migrations
	$(CARGO) run -q -p hollowrite-server -- migrate

seed: migrate ## Validate content; the world needs no seeded rows
	$(CARGO) run -q -p hollowrite-server -- check-content

web-install: ## Install pinned web dependencies
	cd apps/web && $(NPM) ci --no-audit --no-fund

dev: ## Run server and web dev server together
	@scripts/dev.sh

dev-server: ## Run the game server
	$(CARGO) run -p hollowrite-server

dev-web: ## Run the Vite dev server (proxies /api and /ws)
	cd apps/web && $(NPM) run dev

dev-terminal: ## Run the terminal client against SERVER_URL
	$(CARGO) run -q -p hollowrite-terminal -- --server $(SERVER_URL)

build: ## Debug build of everything
	$(CARGO) build --workspace
	cd apps/web && $(NPM) run build

release: ## Optimised server, terminal client and web bundle
	$(CARGO) build --release -p hollowrite-server -p hollowrite-terminal -p hollowrite-test-support
	cd apps/web && $(NPM) run build

test: test-rust test-web ## Unit and simulation tests (no database needed)

test-rust:
	$(CARGO) test --workspace

test-db: ## Database and end-to-end server tests (needs TEST_DATABASE_URL)
	@test -n "$$TEST_DATABASE_URL" || (echo "set TEST_DATABASE_URL (see .env.example)"; exit 1)
	$(CARGO) test -p hollowrite-persistence -p hollowrite-server

test-web:
	cd apps/web && $(NPM) run typecheck && $(NPM) test

lint: ## rustfmt check, clippy and eslint
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	cd apps/web && $(NPM) run lint

fmt: ## Format Rust sources
	$(CARGO) fmt --all

smoke: ## Two bots meet, chat, party and move on a running server
	$(CARGO) run -q -p hollowrite-test-support --bin hollowrite-bot -- --server $(SERVER_URL) smoke

load: ## 20 wandering bots for 30 s against a running server
	$(CARGO) run -q --release -p hollowrite-test-support --bin hollowrite-bot -- --server $(SERVER_URL) load --bots 20 --seconds 30

latency: ## Input-to-ack latency through a 150 ms RTT proxy
	$(CARGO) run -q --release -p hollowrite-test-support --bin hollowrite-bot -- --server $(SERVER_URL) latency --rtt-ms 150 --jitter-ms 40

capture: ## Screenshot the terminal client via tmux (see scripts/capture-terminal.sh)
	@scripts/capture-terminal.sh

clean:
	$(CARGO) clean
	rm -rf apps/web/dist
