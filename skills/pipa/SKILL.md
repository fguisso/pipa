---
name: pipa
description: >-
  Install and drive the `pipa` CLI to deploy/manage static pages on a pipa
  server. Handles installing the CLI if missing, the human-in-the-loop login,
  the access/zone model, and step-up confirmations. Use when asked to deploy a
  site, change a page's access/zone, list/inspect pages, or set up the pipa CLI.
---

# pipa — client skill (agent-agnostic)

This skill drives the `pipa` CLI. It works from any agent (Codex, Claude Code,
Hermes, …). Two facts shape everything:

- **Login and step-up need a human + a browser.** Device-flow approval and
  "loosen security" confirmations cannot be done by the agent. The agent's job
  is to produce the URL and hand it to the human, then wait.
- **Use `--json` for everything you parse.** It suppresses QR codes / spinners /
  colour and prints a single JSON object you can read. Never scrape the human
  output.

Run `pipa concepts` (or `pipa concepts --json`) any time you need the model
spelled out without a network call.

## 0. Ensure the CLI is installed

```sh
pipa --version || curl -fsSL https://guisso.dev/pipa/install.sh | sh
```

If it was just installed, make sure its dir is on `PATH` (the installer prints a
hint; usually `~/.local/bin`).

## 1. Configure / log in (ASK the human for the server URL)

Do **not** guess the server URL — never assume `127.0.0.1`, a public URL, or
anything else. Determine it like this:

1. Check for an existing login:
   ```sh
   pipa whoami --json
   ```
   - `{"logged_in":true,"server":"…"}` → tell the human "you're logged in to
     `<server>` — reuse it?" and only reuse it if they confirm.
   - `{"logged_in":false}` → not logged in.
2. If there's no server to reuse, **ASK the human: "What is the upstream pipa
   server URL?"** Use their exact answer as `<server-url>`.
3. Start the login **without waiting** and hand the URL to the human:
   ```sh
   pipa login --no-wait --json --server <server-url> --label "<device-label>"
   ```
   This prints one JSON object with a `verify_url` and exits immediately. Give
   that URL to the human and ask them to open it in a browser and approve (they
   must already be signed in as the pipa admin/owner).
4. Then **resume** — this blocks until they approve, then stores the token:
   ```sh
   pipa login --resume --json    # returns {"status":"approved",…} on success
   ```
   No background shell, no output scraping: the CLI owns the wait. Re-run
   `--resume` if it ever times out (10 min). Verify with `pipa whoami --json`.

Credentials are stored locally (best tier available, falling back to
`~/.config/pipa/auth.toml`, chmod 600). The chosen server is remembered, so
later commands don't need `--server`.

## 2. Know what the server supports

```sh
pipa server --json   # {"server":"…","features":["zone", …]}
```

`features` lists the **optional** capabilities this server *enforces*. A flag
whose feature is absent is accepted but **not enforced**. The CLI refuses such a
flag by default (see zone below).

## 3. The model (access × zone)

- **access** — *who* can open the page: `password` (default, needs
  `--password <secret>`) or `noauth` (no gate).
- **zone** — *where* it's reachable, an **exact match** (one channel each):
  `private` = served **only** over the internal (LAN) channel; `public` =
  served **only** over the external (internet) channel. Only enforced if the
  server has the `zone` feature.
- New deploys are **secure by default**: `access=password` + the server's
  default zone.

## 4. Deploy

```sh
pipa deploy ./dist --json                                  # access=password, default zone
pipa deploy ./dist --access noauth --json                  # open page
pipa deploy ./dist --zone public --json                    # internet (needs server `zone` feature)
```

- `--zone` against a server without the `zone` feature → the CLI **errors**
  ("would be stored but ignored"). Either drop `--zone`, point at a
  zone-enabled server, or pass `--force` to send it knowingly unenforced.
- Read the JSON result for `uuid`, `url`, `access`, `zone`.

## 5. Change a page (`share`)

Tightening (`--access password`, `--zone private`, `--csp …`) is a plain call:

```sh
pipa share <uuid> --zone private --json
```

**Loosening** (`--access noauth` or `--zone public`) is destructive → step-up.
Use the same `--no-wait` / `--resume` split as login:

```sh
pipa share <uuid> --zone public --no-wait --json   # prints step_up.verify_url, exits
# → hand verify_url to the human to approve in a browser, then:
pipa share <uuid> --zone public --resume --json    # blocks until confirmed, then applies
```

Re-run `--resume` with the **same** loosening flags. Verify with
`pipa get <uuid> --json`.

## 6. Inspect

```sh
pipa ls --json
pipa get <uuid> --json
```

## Gotchas (you won't find these in --help alone)

- Login/step-up approval is **always** a human-in-browser step. Don't loop
  trying to self-approve. The `--no-wait` → `--resume` split is how you hand the
  URL over and then wait — no background shell, no scraping.
- `pipa activity` does not yet have a CLI read endpoint on older servers; the
  audit log lives server-side.
- Deleting (`pipa rm <uuid>`) also needs step-up → same split:
  `pipa rm <uuid> --no-wait --json` then `pipa rm <uuid> --resume --json`.
- Revoking **another** device needs step-up too:
  `pipa devices revoke <id> --no-wait --json` then `… --resume --json`.
- For CI / non-interactive agents, add `--headless`: it never touches the OS
  keychain and never falls back to a file — credentials must come from
  `PIPA_SECRET_GET_CMD`/`PIPA_SECRET_SET_CMD` (an `op`/`bw` command) or
  `PIPA_REFRESH_TOKEN`.
- To install the server itself (not just the client), see `server.md`.
