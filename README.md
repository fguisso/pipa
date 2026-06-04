# pipa

**Self-hosted static-site hosting that fits in your back pocket.** Push a folder of HTML from your laptop and get back a private URL. Add comments, see who visits, password-gate the link, or share it publicly — all from one binary running on a Raspberry Pi.

```
$ pipa deploy ./my-site
✓ deployed
  uuid: 01HXYZ8K4P2NR9M3VQAJWB6CE
  url:  https://pages.example.com/p/01HXYZ8K4P2NR9M3VQAJWB6CE
  size: 1.2 MB
  files: 42
```

That's the whole product.

---

## What's in the box

- **Static + SPA hosting** at `/p/<uuid>/*` with client-side routing fallback.
- **Private by default.** Flip per page to `public`, `password`, or back.
- **Built-in analytics** — views, uniques, top paths, top referrers. No IPs stored.
- **Built-in comments** — anonymous, markdown-sanitized, owner-moderated, embeddable widget.
- **Secure CLI** — device-flow login (QR in terminal), OS-keychain credentials, scoped tokens, browser step-up for destructive ops.
- **Admin dashboard** at `/admin` — Alpine.js, no build step, no framework.
- **One binary.** Rust + axum + SQLite + disk. ~25 MB, runs as non-root, no daemons.
- **Caddy in front for TLS.** The server is HTTP-only by design.

---

## Quickstart — local, 60 seconds

You need [Rust 1.94+](https://www.rust-lang.org/tools/install) (or [`mise`](https://mise.jdx.dev/) to pin it for you).

```sh
git clone https://github.com/fguisso/pipa.git && cd pipa
cargo build --release
cp pages.example.toml pages.toml          # sane defaults; binds 127.0.0.1:8080
```

**Terminal 1 — run the server.** `--dev` relaxes the cookie `Secure` flag for `http://`:

```sh
./target/release/pipa-server --dev
```

On first boot it prints a setup code like `BRZQ-7K9P` (also written to `./data/.setup-code`). Copy it.

**Terminal 2 — log in and deploy.**

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

---

## The CLI

The binary is `pipa`. Every command auto-rotates your refresh token and only requests the smallest scope it needs.

| Command                                | What it does                                            |
|----------------------------------------|---------------------------------------------------------|
| `pipa login [--automation]`           | Device-flow login. `--automation` cannot do destructive ops, period. |
| `pipa whoami`                         | Current device + which credential store you're on.      |
| `pipa deploy <dir> [--uuid X]`        | Zip + upload. Omit `--uuid` to create, pass it to update. |
| `pipa ls`                             | Your pages.                                             |
| `pipa get <uuid>`                     | Metadata for one page.                                  |
| `pipa stats <uuid> [--range 7d]`      | ASCII analytics — views, uniques, top paths, referrers. |
| `pipa share <uuid> --public`          | Flip visibility. `--public` requires step-up.           |
| `pipa share <uuid> --password secret` | Password-gate it.                                       |
| `pipa share <uuid> --csp off`         | Per-page CSP knob. Use `strict` (default) or `off` to let pages load CDN assets. Also accepted on `pipa deploy`. |
| `pipa rm <uuid>`                      | Delete. Always step-up — opens a QR for browser confirm. |
| `pipa devices [revoke <id>]`          | List or revoke logged-in devices.                       |
| `pipa activity --range 7d`            | Recent audit events.                                    |
| `pipa comments enable <uuid>`         | Turn comments on for a page.                            |
| `pipa comments ls <uuid>`             | Moderation queue (visible + pending + hidden).          |

```sh
pipa stats 01HXYZ --range 7d
─── last 7 days ────────────────────────────────────────
views        342    █████████████████░░░░  uniques  87
top paths
  /index.html              198
  /about                    71
top referrers
  google.com                 88
  (direct)                  120
─────────────────────────────────────────────────────────
```

Refresh tokens land in (best available, no silent downgrades):
**macOS Keychain / Windows Credential Manager / libsecret → `pass` → age-encrypted file → `~/.config/pipa/auth.toml` (chmod 600) → `PIPA_REFRESH_TOKEN` env var**.

---

## Comments on a page

Owner side — one CLI call:

```sh
pipa comments enable 01HXYZ                       # turn it on
pipa comments require-approval 01HXYZ --on        # optional: hold new comments for review
```

Reader side — one tag in your HTML:

```html
<script src="http://127.0.0.1:8080/api/comments/widget.js"
        data-page="01HXYZ" async></script>
```

The widget is ~5 KB of vanilla JS — no framework. Renders a form, posts to the API, handles rate limiting (`429 Retry-After`), shows "awaiting moderation" when needed. Markdown is sanitized server-side through `pulldown-cmark + ammonia`: `<script>` is text, links get `rel="nofollow ugc" target="_blank"`, raw HTML is dropped.

Moderate from the terminal:

```sh
pipa comments ls 01HXYZ --status pending
pipa comments approve 01HCMT...A1B2
pipa comments hide    01HCMT...C3D4
```

---

## Admin dashboard

Browse to `http://127.0.0.1:8080/admin`. Paste a refresh token, get a 30-minute signed cookie, see your pages / comments queue / devices / audit log. Live-refreshing via Alpine.js + the same JSON API the CLI uses. No build step, no framework — `ui/public/vendor/alpine.min.js` is the only client-side dependency, served from the binary.

Destructive ops in the admin (delete page, flip to public, revoke another device) intentionally point you back at the CLI — they need the second-device step-up flow, which fits a terminal + phone better than a single-tab browser.

Disable the admin entirely with `[admin] ui_enabled = false` in `pages.toml`.

---

## Production deploy

### Option A — Docker compose (easiest)

```sh
cd docker/
cp ../pages.example.toml ./pages.toml
$EDITOR ./Caddyfile          # set your domain + Let's Encrypt email
docker compose up -d
```

Multi-arch image (`amd64` / `arm64`) built from [`docker/Dockerfile`](docker/Dockerfile). Only Caddy is exposed on the host (`:80`, `:443`); `pipa-server` stays on the internal bridge. State persists in `docker/data/` and `docker/pages/`.

### Option B — install script + systemd + Caddy

```sh
curl -fsSL https://raw.githubusercontent.com/fguisso/pipa/main/packaging/install.sh \
  | sudo bash
```

The script downloads the right binary for your arch, verifies its SHA-256, creates a `pipa` system user, drops `/etc/pipa/pages.toml`, and installs a hardened systemd unit. Pin a release with `sudo PIPA_VERSION=v0.1.0 bash install.sh`.

Then put Caddy in front:

```sh
sudo apt install caddy
sudo cp Caddyfile.example /etc/caddy/Caddyfile
sudo $EDITOR /etc/caddy/Caddyfile      # domain + Let's Encrypt email
sudo $EDITOR /etc/pipa/pages.toml     # public_url, trusted_proxy
sudo systemctl reload caddy
sudo systemctl enable --now pipa-server
```

### Option C — build from source

```sh
cargo build --release -p pipa-server
sudo install -m 755 target/release/pipa-server /usr/local/bin/
sudo install -m 644 packaging/systemd/pipa-server.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now pipa-server
```

---

## Architecture

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
| TLS         | External proxy (Caddy default) — **mandatory in prod** |

Both storage layers sit behind traits (`Storage`, `Repository`, `AuthStore`) so a future Phase 5 can swap SQLite → Postgres and disk → S3 without touching business logic.

External TLS is mandatory because `pipa-server` deliberately doesn't embed it — the binary stays small and runs as a non-root user. See [`adr/0005-tls-termination.md`](.claude/project-docs/adr/0005-tls-termination.md).

---

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

---

## Development

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

Acceptance matrix for Phase 1 lives in [`ACCEPTANCE.md`](ACCEPTANCE.md).

---

## Configuration

All knobs live in [`pages.example.toml`](pages.example.toml), grouped by section: `[server]`, `[hosting]`, `[analytics]`, `[admin]`, `[auth]`, `[auth.notifications]`, `[comments]`. Every field is optional and inline-documented; copy the file, edit the bits you care about, ignore the rest.

Defaults work out of the box for local dev (`127.0.0.1:8080`, SQLite under `./data/`, page bundles under `./pages/`).

---

## Security

- TLS is the proxy's job — `pipa-server` listens HTTP only and refuses non-loopback in `--dev`.
- Default response headers: strict CSP on hosted pages, `nosniff`, conservative `Permissions-Policy`, no `Server` header. The default CSP can be opted out per-page (`--csp off` on deploy/share) for sites that legitimately load CDN assets — those pages then declare their own policy via `<meta http-equiv>`.
- Passwords: argon2id (64 MiB, t=3, p=1). Tokens: HMAC, hashed at rest, rotated on every use.
- Uploads: path-traversal blocked, symlinks rejected, exec bit dropped, 100 MB cap, zip-bomb guarded.
- Destructive ops always require a fresh second-device confirmation (QR or URL).
- Audit log is append-only; every authenticated mutation lands a row.

Read [`SECURITY.md`](.claude/project-docs/SECURITY.md) for the always-on defaults and the threat model.

---

## License

Copyright (C) 2026 Fernando Guisso.

- Server, core, adapters, and CLI: **AGPL-3.0-or-later** ([`LICENSE`](LICENSE))
- SDK: **Apache-2.0** ([`crates/pipa-sdk/LICENSE`](crates/pipa-sdk/LICENSE)) — kept permissive so third-party tools and AI agents can embed the API client without copyleft obligations.

Contribute via DCO sign-off (`git commit -s`). No CLA.
