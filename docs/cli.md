# The CLI

The binary is `pipa`. Every command auto-rotates your refresh token and only requests the smallest scope it needs.

Add `--json` to any command for machine-readable output (no QR, colour, or spinners) — use it whenever a script or an agent parses the result. `pipa login --json` prints the verify URL as a single JSON object so you don't have to scrape the terminal.

| Command                                          | What it does                                                                 |
|--------------------------------------------------|------------------------------------------------------------------------------|
| `pipa login [--automation]`                     | Device-flow login (approve in a browser). `--automation` cannot do destructive ops, period. |
| `pipa whoami`                                    | Current device, server, and which credential store you're on.                |
| `pipa server`                                    | The target server's URL and the optional features it enforces (e.g. `zone`). |
| `pipa concepts`                                  | Prints the access × zone model offline (no network).                         |
| `pipa deploy <dir> [--uuid X]`                   | Zip and upload. Omit `--uuid` to create, pass it to update.                   |
| `pipa ls`                                         | Your pages.                                                                  |
| `pipa get <uuid>`                                | Metadata for one page.                                                       |
| `pipa stats <uuid> [--range 7d]`                 | ASCII analytics: views, uniques, top paths, referrers.                       |
| `pipa share <uuid> --access password\|noauth`   | Change *who* can open it. `--access noauth` (drop the gate) requires step-up. |
| `pipa share <uuid> --zone public\|private`      | Change *where* it's reachable. `--zone public` requires step-up. Needs the server's `zone` feature. |
| `pipa share <uuid> --csp off`                    | Per-page CSP knob. `strict` (default) or `off` to let pages load CDN assets. Also on `pipa deploy`. |
| `pipa rm <uuid>`                                 | Delete. Always step-up, opens a QR for browser confirm.                       |
| `pipa devices [revoke <id>]`                     | List or revoke logged-in devices.                                            |
| `pipa activity --range 7d`                       | Recent audit events.                                                         |
| `pipa comments enable <uuid>`                    | Turn comments on for a page.                                                 |
| `pipa comments ls <uuid>`                        | Moderation queue (visible, pending, hidden).                                |

## access × zone

Two orthogonal axes (run `pipa concepts` for the canonical version):

- **access** — *who* can open the page: `password` (default; pass `--password <secret>` on deploy) or `noauth` (no gate).
- **zone** — *where* it's reachable, an **exact match** (one channel each): `private` is served **only** over the internal (LAN) channel; `public` **only** over the external (internet) channel. Enforced only when the server is built with the `zone` feature.

New deploys are secure by default: `access=password` plus the server's configured default zone.

```sh
pipa deploy ./dist                         # password-gated, default zone
pipa deploy ./dist --access noauth          # open to anyone who can reach it
pipa deploy ./dist --zone public            # internet-reachable (needs server `zone` feature)
```

**Feature gating.** Flags whose feature the target server doesn't enforce are refused by the CLI (it checks `pipa server`) so you don't get a false sense of security. Pass `--force` to send the value anyway, knowing it won't be enforced.

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

Refresh tokens land in the best available store, with no silent downgrades:
**macOS Keychain / Windows Credential Manager / libsecret → `pass` → age-encrypted file → `~/.config/pipa/auth.toml` (chmod 600) → `PIPA_REFRESH_TOKEN` env var**.
