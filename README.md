# gapes

Self-hosted static site + SPA hosting, with built-in comments and analytics, served behind UUID URLs. Designed to run on a Raspberry Pi and scale to SaaS from the same codebase.

> Status: scaffold / Phase 1 in design. Not yet runnable.

## Stack

- **Server**: Rust + axum + tokio + sqlx (SQLite) — `crates/gapes-server`
- **CLI**: Rust + clap — `crates/gapes-cli`, ships as binary `gapes`
- **UI**: askama templates + HTMX + Alpine.js, no build step
- **Storage**: disk + SQLite in Phase 1; pluggable to S3 + Postgres in Phase 5
- **TLS**: Caddy in front (separate process)

## Repo layout

```
crates/
├── gapes-core/         business logic, no I/O           (AGPL-3.0)
├── gapes-adapters/     storage / db / auth impls        (AGPL-3.0)
├── gapes-server/       axum binary                      (AGPL-3.0)
├── gapes-cli/          CLI binary `gapes`               (AGPL-3.0)
└── gapes-sdk/          API client library               (Apache-2.0)
ui/
├── templates/          askama templates
├── public/             htmx, alpine, css, fonts
└── widget/             comments widget (vanilla JS)
```

## License

- Server, core, adapters, and CLI: **AGPL-3.0-or-later** (see [LICENSE](LICENSE))
- SDK only: **Apache-2.0** (see [crates/gapes-sdk/LICENSE](crates/gapes-sdk/LICENSE)) — kept permissive so third-party tools and AI agents can embed the API client without copyleft obligations
- Contributions via DCO (`git commit -s`). No CLA.

## Dev setup

Requires [mise](https://mise.jdx.dev/). The project pins its Rust toolchain in `mise.toml`.

```sh
mise install                 # pin Rust toolchain
cargo install sqlx-cli cargo-watch   # one-time
mise run check               # type-check the workspace
mise run dev                 # run the server with auto-reload
```
