# Development

```sh
mise install                                    # pins Rust 1.94
cargo check --workspace --all-targets           # type-check everything
cargo test  --workspace                         # 73 tests
cargo build --workspace                         # debug build
cargo build --release -p pipa-server           # release binary
```

## Optional server features

`pipa-server` has opt-in Cargo features (all off by default):

- `zone` — enforce per-page network reach (`private`/`public`); needs the `[zone]` config + an internal proxy.
- `thumbnails` — screenshot each page for the admin dashboard. Build with `cargo build --release -p pipa-server --features thumbnails`. It shells out to headless Chromium, so the operator must install a Chromium/Chrome binary and point `[thumbnails].chromium_path` at it; configure `[thumbnails]` (`enabled`, `chromium_path`, `width`, `height`). Missing browser → no thumbnail, deploys unaffected.

Migrations live in `crates/pipa-adapters/migrations/` and run on startup. Recent additions: `0009` (analytics `hits.kind`), `0010` (users/sessions/oauth + `devices.user_id`), `0011` (workspaces + members; seeds a personal workspace per user).

Or with [`mise`](https://mise.jdx.dev/) tasks:

```sh
mise run check          # type-check
mise run test           # tests
mise run dev            # cargo-watch run -p pipa-server
mise run lint           # clippy -D warnings
mise run fmt            # rustfmt
```

Acceptance matrix for Phase 1 lives in [`ACCEPTANCE.md`](../ACCEPTANCE.md).

## Configuration

All knobs live in [`pages.example.toml`](../pages.example.toml), grouped by section: `[server]`, `[hosting]`, `[analytics]`, `[admin]`, `[auth]`, `[auth.notifications]`, `[comments]`. Every field is optional and inline-documented; copy the file, edit the bits you care about, ignore the rest.

Defaults work out of the box for local dev (`127.0.0.1:8080`, SQLite under `./data/`, page bundles under `./pages/`).
