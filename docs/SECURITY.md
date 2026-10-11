# Security

Scope: the v0 local slice and what has to change before anything is exposed
to the internet. Report vulnerabilities privately to the repository owner
rather than in public issues.

## Threat model in one paragraph

Players control their clients completely. Anything a client sends is
untrusted: positions, damage, item ids, targets, names, chat, sequence
numbers and frame sizes. Attackers may also be other websites (cross-site
requests from a logged-in browser), other players (chat injection,
impersonation, harassment, griefing shared fights) or people guessing
passwords. The server must stay authoritative and available under all of
these.

## Controls in place

### Authority

- Clients send intent only: held direction, button presses, menu choices
  and chat. The server owns position, collision, health, stamina, wick,
  cooldowns, enemy AI, RNG, loot, rewards, quests and progression.
- Every command is checked against the character's current state on the
  world actor: equipment must be owned and fit the slot and requirements;
  consumables must be held; levelling needs a nearby lit lantern and enough
  echoes; dialogue choices must belong to the node last shown, conditions are
  rechecked, and item takes are verified before anything is granted.
- Unknown message types and unknown enum values fail deserialisation and are
  rejected. Extra fields are ignored and never acted on (the end-to-end
  test sends a fake `teleport` field and a fake `grant_echoes` command).
- Input sequence numbers must increase; duplicates and replays are dropped.
- Characters are scoped to their account in every query
  (`where id = $1 and account_id = $2`).

### Accounts and sessions

- Passwords: Argon2id with the `argon2` crate defaults (19 MiB, 2 passes, 1
  lane, random salt), hashed on a blocking thread. 10–128 characters, must
  differ from the name.
- Unknown names cost the same Argon2 verification as wrong passwords, so
  timing does not reveal which names exist; the error text is identical.
- Lockout: 5 consecutive failures lock the account for 5 minutes. Rate
  limits: registrations per IP per hour (configurable), 20 logins/min per IP,
  8/min per name, 1 WebSocket connect per second per IP (burst 20).
- Session tokens: 32 random bytes from the OS, base64url. Only the SHA-256
  of the token is stored. Sessions expire (14 days by default), are revoked
  on logout, and are revoked for the whole account on suspension. Expired
  sessions are purged after a week.
- Browser: token in an `HttpOnly; SameSite=Strict; Path=/` cookie (`Secure`
  when `HOLLOWRITE_SECURE_COOKIES=1`). It is never returned in a response
  body and never stored in `localStorage`.
- Terminal: token returned in the login response, stored at
  `~/.config/hollowrite/session.json` with mode 0600 (Unix), removed on
  logout; `--no-remember` keeps it in memory only. The password itself is
  never written anywhere and is cleared from memory after the request.
  Tokens are never placed in URLs.

### Cross-site protections

- Every state-changing HTTP request requires the custom header
  `x-hollowrite`, which a foreign site cannot add without a CORS preflight,
  and the server never answers CORS preflights. If an `Origin` header is
  present it must be in the allow-list.
- WebSocket upgrades from browsers must carry an allowed `Origin`;
  upgrades without an `Origin` must use a bearer token and cookies are
  ignored. A malicious page can therefore not open a game socket with the
  player's cookie.
- Responses carry `Content-Security-Policy` (`default-src 'self'`, no inline
  script, `frame-ancestors 'none'`), `X-Frame-Options: DENY`,
  `X-Content-Type-Options: nosniff` and `Referrer-Policy: no-referrer`.

### Text and injection

- Names: ASCII letters, digits, `-` and `_`, leading letter, 3–16
  characters, unique ignoring case (database index). ASCII-only rules out
  homoglyph impersonation and keeps every name single-width.
- Chat and report text pass through `sanitize_text`: control characters
  (including ESC and BEL), bidi overrides and zero-width characters are
  removed, whitespace is collapsed and length is capped at 200.
- Clients sanitise again before display. The terminal client never writes
  raw strings to the terminal (Ratatui renders into cells). The web client
  renders text through React and the canvas; `dangerouslySetInnerHTML` is
  banned by lint.
- SQL uses bound parameters everywhere.

### Abuse and moderation

- Chat: 5-message burst then 1 per second per connection; local chat reaches
  players within 24 cells, party chat only members.
- `/mute name` hides that account's chat for you (persisted per account).
- `/report name reason` stores a report and logs it under the `moderation`
  target; `hollowrite-server admin reports` lists open reports.
- `hollowrite-server admin suspend <name> --hours N --reason ...` suspends,
  revokes all sessions and is written to `audit_log`; the running server
  disconnects suspended accounts within 15 seconds. `unsuspend` and
  `promote` are audited the same way.
- Boss griefing: the arena seals five seconds after the fight starts, so
  outsiders cannot join, drag adds in, or steal the kill; non-participants
  never get rewards; dead participants drop out.
- Loot is personal: drops are rolled per contributor and visible only to
  their owner, so there is nothing to steal.

### Resource limits

Frame size (4 KiB), per-connection frame rate, malformed-frame strikes,
hello and idle timeouts, bounded per-connection and world queues with
slow-consumer disconnects, at most four characters per account (enforced
under a row lock), at most four party members and eight pending invites.

### Data stored

| data | why | PII |
|---|---|---|
| account name | login, display | pseudonymous identifier |
| Argon2id hash | login | secret, never logged |
| session token hashes, timestamps, client kind | sessions | low |
| characters and ledger | game state | no |
| mutes, reports (with free text) | moderation | reports may contain personal data written by players |
| audit log | admin actions | admin identifiers |

No e-mail addresses or IP addresses are stored in the database. IP
addresses appear in logs at connection time and in in-memory rate limiters.
Logs never contain passwords, tokens or token hashes.

## Known gaps before any public deployment

These are **not** done and must not be assumed:

- TLS termination (run behind Caddy, nginx or a load balancer; set
  `HOLLOWRITE_SECURE_COOKIES=1` and `HOLLOWRITE_ALLOWED_ORIGINS`).
- Verified e-mail and password reset. Without them a forgotten password
  means a lost account.
- Privacy notice, terms of play and a moderation policy; a retention period
  for reports and logs.
- An admin web interface (moderation is CLI-only) and alerting on reports.
- Automated backups and restore drills (OPERATIONS.md describes the manual
  procedure).
- Production secret management for `DATABASE_URL`.
- Abuse monitoring and IP reputation; the rate limiters are per-process and
  reset on restart.
- Rate limits are keyed by the socket peer address. Behind a reverse proxy
  every client shares the proxy's address; trusted forwarding headers need
  to be added before deploying that way.
- A dependency review process beyond `cargo audit` and `npm audit` in CI.
