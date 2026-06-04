# Development

```sh
mise install                                    # pins Rust 1.94
cargo check --workspace --all-targets           # type-check everything
cargo test  --workspace                         # 50 tests, ~6s
cargo build --workspace                         # debug build
cargo build --release -p pipa-server           # release binary
```

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
