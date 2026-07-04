# The CLI

The binary is `pipa`. Every command auto-rotates your refresh token and only requests the smallest scope it needs.

Add `--json` to any command for machine-readable output (no QR, colour, or spinners) — use it whenever a script or an agent parses the result. `pipa login --json`, `pipa stats --json`, etc. print a single JSON object so you don't have to scrape the terminal.

Add `--headless` (global) for CI / containers / agents: it never touches the OS keychain and never falls back to an on-disk file — the credential must come from an external command vault or `PIPA_REFRESH_TOKEN` (see [Credentials](#credentials)) — and it suppresses browser-open and interactive prompts, so a locked keyring or a missing TTY can't hang the CLI.

| Command                                          | What it does                                                                 |
|--------------------------------------------------|------------------------------------------------------------------------------|
| `pipa login [--automation]`                     | Device-flow login (approve in a browser). `--automation` cannot do destructive ops, period. `--no-wait` / `--resume` split the flow (see [Login](#login-and-step-up)). |
| `pipa whoami`                                    | Current device, server, and which credential store you're on.                |
| `pipa server`                                    | The target server's URL and the optional features it enforces (e.g. `zone`). |
| `pipa concepts`                                  | Prints the access × zone model offline (no network).                         |
| `pipa deploy <dir> [--uuid X] [--new] [--workspace W]` | Zip and upload. With no flag it *updates* the page it remembers for this directory (else creates one); `--new` forces a fresh page; `--workspace` targets a workspace (else the active/personal one). |
| `pipa ls`                                         | Your pages (a user: across all its workspaces; the local operator: all pages). |
| `pipa get <uuid>`                                | Metadata for one page.                                                       |
| `pipa stats <uuid> [--range 7d]`                 | Analytics: page views, uniques, top paths, referrers. `--json` for raw data. |
| `pipa share <uuid> --access password\|noauth`   | Change *who* can open it. `--access noauth` (drop the gate) requires step-up. |
| `pipa share <uuid> --zone public\|private`      | Change *where* it's reachable. `--zone public` requires step-up. Needs the server's `zone` feature. |
| `pipa share <uuid> --csp off`                    | Per-page CSP knob. `strict` (default) or `off` to let pages load CDN assets. Also on `pipa deploy`. |
| `pipa rm <uuid>`                                 | Delete. Always step-up (opens a browser confirm); `--no-wait`/`--resume` to split it. |
| `pipa transfer <uuid> <workspace>`              | Move a page to another workspace (needs edit rights on both; quota-checked).  |
| `pipa workspace ls\|use\|unset\|create\|show\|member-add\|member-role\|member-rm\|quota` | Manage workspaces & membership (see [Workspaces](#workspaces)). |
| `pipa devices [revoke <id>]`                     | List or revoke logged-in devices. Revoking *another* device is step-up.       |
| `pipa activity --range 7d`                       | Recent audit events.                                                         |
| `pipa comments enable <uuid>`                    | Turn comments on for a page.                                                 |
| `pipa comments ls <uuid>`                        | Moderation queue (visible, pending, hidden).                                |

## Login and step-up

Split with `--no-wait` / `--resume`.

Login and destructive-op confirmation always need a human in a browser — the CLI can never self-approve. The default `pipa login` opens a browser and blocks until you approve. For agents (or any caller that must hand the URL over *before* waiting), split it in two — no backgrounded shell, no output scraping:

```sh
pipa login --no-wait --json --server http://127.0.0.1:8080   # prints {"verify_url":…} and exits
# → give verify_url to a human to approve in a browser, then:
pipa login --resume --json                                    # blocks until approved, then stores the token
```

The same `--no-wait` / `--resume` split works for every step-up operation — `pipa rm <uuid>`, `pipa share <uuid> --access noauth` (loosening), and `pipa devices revoke <other-id>`: run it with `--no-wait` to print the confirmation URL and exit, then re-run the **same** command with `--resume` to wait for the human and execute.

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

## Workspaces

Every user has a **personal workspace**; pages are owned by a workspace, and members act according to their role: `owner` and `admin` manage members and settings, `editor` deploys/edits/deletes pages, `viewer` reads only. Non-members get a 403.

```sh
pipa workspace ls                          # workspaces you belong to + your role in each
pipa workspace create "acme"               # new team workspace (you become owner)
pipa workspace use <ws-id>                 # set the active workspace (persisted in config)
pipa workspace unset                       # back to your personal workspace
pipa workspace show [<ws-id>]              # members + quota (defaults to the active one)
pipa workspace member-add <ws> <username> --role editor
pipa workspace member-role <ws> <user-id> editor
pipa workspace member-rm  <ws> <user-id>
pipa workspace quota <ws> --max-pages 50 --max-bytes 1073741824   # omit a flag to leave it
```

`pipa deploy` targets the active workspace (or `--workspace <id>` to override); over-quota creates/transfers are refused with `quota_exceeded`.

## Credentials

Refresh tokens land in the best available store, with no silent downgrades:
**`PIPA_REFRESH_TOKEN` env var → external command vault → macOS Keychain / Windows Credential Manager / libsecret → `pass` → age-encrypted file → `~/.config/pipa/auth.toml` (chmod 600)**.

**External command vault (1Password / Bitwarden / any tool).** Point the CLI at your password manager with two env vars, each a shell command:

```sh
export PIPA_SECRET_GET_CMD='op read op://Private/pipa/refresh'   # prints the token to stdout
export PIPA_SECRET_SET_CMD='op item edit …'                     # reads the token on stdin (optional)
```

The target server URL is exported to the command as `PIPA_SECRET_SERVER` so one template can key items per server. If `PIPA_SECRET_SET_CMD` is unset the vault is read-only (rotations are surfaced, not persisted), like `PIPA_REFRESH_TOKEN`.

**`--headless`** restricts credential resolution to the command vault and `PIPA_REFRESH_TOKEN` only — it **never** touches the OS keychain and **never** falls back to an on-disk file; if neither is configured it fails loudly instead of hanging on a locked keyring. Use it in CI, containers, and non-interactive agents.
