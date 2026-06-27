# gapes

**Self-hosted static-site hosting that fits in your back pocket.** Push a folder of HTML from your laptop and get back a private URL. Add comments, see who visits, password-gate the link, or share it publicly, all from one binary running on a Raspberry Pi.

```
$ gapes deploy ./my-site
✓ deployed
  uuid: 01HXYZ8K4P2NR9M3VQAJWB6CE
  url:  https://pages.example.com/p/01HXYZ8K4P2NR9M3VQAJWB6CE
  size: 1.2 MB
  files: 42
```

That's the whole product.

## What's in the box

- **Static + SPA hosting** at `/p/<uuid>/*` with client-side routing fallback.
- **Secure by default.** New pages are password-gated. Per page you set *who* can open it (`access`: `password` / `noauth`) and *where* it's reachable (`zone`: `private` = LAN only / `public` = internet only).
- **Built-in analytics:** views, uniques, top paths, top referrers. No IPs stored.
- **Built-in comments:** anonymous, markdown-sanitized, owner-moderated, embeddable widget.
- **Secure CLI:** device-flow login (QR in terminal), OS-keychain credentials, scoped tokens, browser step-up for destructive ops.
- **Agent-ready.** `--json` output everywhere, capability discovery (`gapes server`), and an installable [agent skill](skills/gapes/SKILL.md) so an AI agent can install and drive gapes.
- **Admin dashboard** at `/admin`: Alpine.js, no build step, no framework.
- **One binary.** Rust + axum + SQLite + disk. ~25 MB, runs as non-root, no daemons.
- **Caddy in front for TLS.** The server is HTTP-only by design.

## Quickstart (local, 60 seconds)

Install the CLI (users): `curl -fsSL https://guisso.dev/gapes/install.sh | sh`.

To run the server locally from source you need [Rust 1.94+](https://www.rust-lang.org/tools/install) (or [`mise`](https://mise.jdx.dev/) to pin it for you).

```sh
git clone https://github.com/fguisso/gapes.git && cd gapes
cargo build --release
cp pages.example.toml pages.toml          # sane defaults; binds 127.0.0.1:8080
```

**Terminal 1, run the server.** `--dev` relaxes the cookie `Secure` flag for `http://`:

```sh
./target/release/gapes-server --dev
```

On first boot, open `http://127.0.0.1:8080/setup` in a browser and create your admin account.

**Terminal 2, log in and deploy.**

```sh
gapes login --server http://127.0.0.1:8080
#   ► visit http://127.0.0.1:8080/cli on any device
#   ► or scan the QR
#   ► open it, sign in as admin, and approve the device

echo '<h1>hello from gapes</h1>' > /tmp/site/index.html
gapes deploy /tmp/site          # secure by default: password-gated, server's default zone
# ✓ deployed
#   url: http://127.0.0.1:8080/p/01HXYZ...
```

Open it to anyone (`gapes share <uuid> --access noauth`) or expose it to the internet (`gapes share <uuid> --zone public`); each loosens security, so each needs a browser step-up. Run `gapes concepts` for the full model.

## Documentation

- [The CLI](docs/cli.md): every command, `--json`, scopes, and where refresh tokens land.
- [Agent skill](skills/gapes/SKILL.md): install and drive gapes from any AI agent (Codex / Claude Code / Hermes).
- [Comments on a page](docs/comments.md): enable, embed the widget, moderate.
- [Admin dashboard](docs/admin.md): the `/admin` UI and what it can and can't do.
- [Production deploy](docs/deploy.md): Docker compose, install script, or from source.
- [Architecture](docs/architecture.md): the stack, storage traits, and repo layout.
- [Development](docs/development.md): build, test, `mise` tasks, configuration.
- [Security](docs/security.md): always-on defaults and the threat model.

## License

Copyright (C) 2026 Fernando Guisso.

- Server, core, adapters, and CLI: **AGPL-3.0-or-later** ([`LICENSE`](LICENSE))
- SDK: **Apache-2.0** ([`crates/gapes-sdk/LICENSE`](crates/gapes-sdk/LICENSE)), kept permissive so third-party tools and AI agents can embed the API client without copyleft obligations.

Contribute via DCO sign-off (`git commit -s`). No CLA.
