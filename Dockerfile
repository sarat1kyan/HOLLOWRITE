# Server image with the web client baked in.
FROM node:22.12-bookworm-slim AS web
WORKDIR /src/apps/web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY apps/web ./
COPY content /src/content
RUN npm run build

FROM rust:1.88-bookworm AS server
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p hollowrite-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /srv/hollowrite hollowrite
WORKDIR /srv/hollowrite
COPY --from=server /src/target/release/hollowrite-server /usr/local/bin/hollowrite-server
COPY content ./content
COPY --from=web /src/apps/web/dist ./web
ENV HOLLOWRITE_CONTENT_DIR=/srv/hollowrite/content HOLLOWRITE_WEB_DIR=/srv/hollowrite/web
USER hollowrite
EXPOSE 7700
HEALTHCHECK --interval=10s --timeout=3s CMD curl -fsS http://127.0.0.1:7700/healthz || exit 1
ENTRYPOINT ["hollowrite-server"]
CMD ["serve"]
