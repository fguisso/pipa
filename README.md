# pipa

**Self-hosted static-site hosting that fits in your back pocket.** Push a folder of HTML from your laptop and get back a private URL. Add comments, see who visits, password-gate the link, or share it publicly, all from one binary running on a Raspberry Pi.

```
$ pipa deploy ./my-site
✓ deployed
  uuid: 01HXYZ8K4P2NR9M3VQAJWB6CE
  url:  https://pages.example.com/p/01HXYZ8K4P2NR9M3VQAJWB6CE
  size: 1.2 MB
  files: 42
```

That's the whole product.

## What's in the box

- **Static + SPA hosting** at `/p/<uuid>/*` with client-side routing fallback.
- **Private by default.** Flip per page to `public`, `password`, or back.
- **Built-in analytics:** views, uniques, top paths, top referrers. No IPs stored.
- **Built-in comments:** anonymous, markdown-sanitized, owner-moderated, embeddable widget.
- **Secure CLI:** device-flow login (QR in terminal), OS-keychain credentials, scoped tokens, browser step-up for destructive ops.
- **Admin dashboard** at `/admin`: Alpine.js, no build step, no framework.
- **One binary.** Rust + axum + SQLite + disk. ~25 MB, runs as non-root, no daemons.
- **Caddy in front for TLS.** The server is HTTP-only by design.

## Quickstart (local, 60 seconds)

You need [Rust 1.94+](https://www.rust-lang.org/tools/install) (or [`mise`](https://mise.jdx.dev/) to pin it for you).

```sh
git clone https://github.com/fguisso/pipa.git && cd pipa
cargo build --release
cp pages.example.toml pages.toml          # sane defaults; binds 127.0.0.1:8080
```

**Terminal 1, run the server.** `--dev` relaxes the cookie `Secure` flag for `http://`:

```sh
./target/release/pipa-server --dev
```

On first boot it prints a setup code like `BRZQ-7K9P` (also written to `./data/.setup-code`). Copy it.

**Terminal 2, log in and deploy.**

```sh
./target/release/pipa login --server http://127.0.0.1:8080
#   ► visit http://127.0.0.1:8080/cli on any device
#   ► or scan the QR
#   ► paste the setup code (BRZQ-7K9P) and click approve

echo '<h1>hello from pipa</h1>' > /tmp/site/index.html
./target/release/pipa deploy /tmp/site --visibility public
# ✓ deployed
#   url: http://127.0.0.1:8080/p/01HXYZ...
```

Open the URL. That's it.

## Documentation

- [The CLI](docs/cli.md): every command, scopes, and where refresh tokens land.
- [Comments on a page](docs/comments.md): enable, embed the widget, moderate.
- [Admin dashboard](docs/admin.md): the `/admin` UI and what it can and can't do.
- [Production deploy](docs/deploy.md): Docker compose, install script, or from source.
- [Architecture](docs/architecture.md): the stack, storage traits, and repo layout.
- [Development](docs/development.md): build, test, `mise` tasks, configuration.
- [Security](docs/security.md): always-on defaults and the threat model.

## License

Copyright (C) 2026 Fernando Guisso.

- Server, core, adapters, and CLI: **AGPL-3.0-or-later** ([`LICENSE`](LICENSE))
- SDK: **Apache-2.0** ([`crates/pipa-sdk/LICENSE`](crates/pipa-sdk/LICENSE)), kept permissive so third-party tools and AI agents can embed the API client without copyleft obligations.

Contribute via DCO sign-off (`git commit -s`). No CLA.
