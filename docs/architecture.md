# Architecture

```
   internet :443 ──► Caddy (TLS) ──► pipa-server :8080 ──► SQLite + disk
                                       HTTP-only, non-root
```

| Layer       | Choice                                |
|-------------|---------------------------------------|
| Server      | Rust + axum + tokio + sqlx            |
| HTML render | askama (Jinja-style, compile-checked) |
| UI          | Alpine.js, no build step              |
| Storage     | SQLite + local disk (Phase 1)         |
| Auth        | Device flow + scoped HMAC tokens + step-up |
| TLS         | External proxy (Caddy default), **mandatory in prod** |

Both storage layers sit behind traits (`Storage`, `Repository`, `AuthStore`) so a future Phase 5 can swap SQLite for Postgres and disk for S3 without touching business logic.

External TLS is mandatory because `pipa-server` deliberately doesn't embed it. The binary stays small and runs as a non-root user (see ADR 0005: TLS termination).

## Identity & ownership model

Pages carry an `(owner_kind, owner_id)` pair. Three owner kinds coexist:

- **`local`** — the single server-operator (Phase 1). Its pages stay `owner_kind='local'`; the operator is a superuser over everything.
- **`user`** — a signed-up account (Phase 3). Legacy remnant; migration 0011 moves user-owned pages to workspaces.
- **`workspace`** — the normal case for accounts (Phase 4). Every user has a personal workspace; teams are additional workspaces. A caller's authority is resolved from its device → user → workspace membership → role (`owner`/`admin`/`editor`/`viewer`).

Auth/data tables added on top of the Phase-1 schema (`devices`, `refresh_tokens`, `admins`, `owner_sessions`, …):

- `users`, `user_sessions`, `oauth_identities`, and `devices.user_id` — migration `0010` (multi-user; OAuth tables are scaffold only).
- `workspaces`, `workspace_members` — migration `0011` (each user seeded a personal workspace; user-owned pages migrated to workspace ownership).
- `hits.kind` — migration `0009` (page-view vs. asset, so analytics counts navigations, not sub-resources).

## Thumbnails (optional feature)

Built with `--features thumbnails` (off by default), `pipa-server` screenshots each page after deploy for the admin dashboard. Capture is deliberately isolated: it spins a throwaway static server bound to `127.0.0.1:0` over the page's on-disk bundle, points a headless Chromium at it once, then tears it down — the public serve path is never involved and there is no auth-bypass token, so even password-gated pages screenshot their real content. PNGs live at `<data_dir>/thumbnails/<uuid>.png` (outside the bundle) and are served admin-only. Requires a Chromium/Chrome binary; missing or slow capture degrades to no thumbnail and never affects a deploy.

## Repo layout

```
crates/
├── pipa-core/         business logic, no I/O           (AGPL-3.0)
├── pipa-adapters/     SQLite + disk + crypto + config  (AGPL-3.0)
├── pipa-server/       axum binary `pipa-server`       (AGPL-3.0)
├── pipa-cli/          CLI binary `pipa`               (AGPL-3.0)
└── pipa-sdk/          HTTP client library              (Apache-2.0)
ui/
├── templates/          (askama, in-binary)
├── public/             alpine, admin assets
└── widget/             comments widget (vanilla JS)
docker/                 Dockerfile + compose + Caddyfile
packaging/              systemd unit + install.sh
```
