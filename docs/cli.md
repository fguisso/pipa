# The CLI

The binary is `pipa`. Every command auto-rotates your refresh token and only requests the smallest scope it needs.

| Command                                | What it does                                            |
|----------------------------------------|---------------------------------------------------------|
| `pipa login [--automation]`           | Device-flow login. `--automation` cannot do destructive ops, period. |
| `pipa whoami`                         | Current device plus which credential store you're on.   |
| `pipa deploy <dir> [--uuid X]`        | Zip and upload. Omit `--uuid` to create, pass it to update. |
| `pipa ls`                             | Your pages.                                             |
| `pipa get <uuid>`                     | Metadata for one page.                                  |
| `pipa stats <uuid> [--range 7d]`      | ASCII analytics: views, uniques, top paths, referrers.  |
| `pipa share <uuid> --public`          | Flip visibility. `--public` requires step-up.           |
| `pipa share <uuid> --password secret` | Password-gate it.                                       |
| `pipa share <uuid> --csp off`         | Per-page CSP knob. Use `strict` (default) or `off` to let pages load CDN assets. Also accepted on `pipa deploy`. |
| `pipa rm <uuid>`                      | Delete. Always step-up, opens a QR for browser confirm. |
| `pipa devices [revoke <id>]`          | List or revoke logged-in devices.                       |
| `pipa activity --range 7d`            | Recent audit events.                                    |
| `pipa comments enable <uuid>`         | Turn comments on for a page.                            |
| `pipa comments ls <uuid>`             | Moderation queue (visible, pending, hidden).            |

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
