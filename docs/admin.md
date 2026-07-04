# Admin dashboard

Browse to `http://127.0.0.1:8080/admin`. On first boot, `/setup` walks you through creating the server-operator admin account (username + password); after that, `/admin/login` signs you in and issues a signed session cookie. You get your pages, comments queue, devices, and audit log, live-refreshed via Alpine.js and the same JSON API the CLI uses. No build step, no framework: `alpine.min.js` is the only client-side dependency, served from the binary.

The operator is the **"local" superuser** — it can see and act on every page, regardless of which user or workspace owns it.

Destructive ops in the admin (delete page, flip to public, revoke another device) intentionally point you back at the CLI. They need the second-device step-up flow, which fits a terminal plus phone better than a single-tab browser.

Disable the admin entirely with `[admin] ui_enabled = false` in `pages.toml`.

## Accounts & workspaces (multi-user)

Beyond the operator, regular users sign up and manage their own pages through their own signed session (`pipa_user` cookie), separate from the operator's admin cookie:

- **`/signup`** — create an account (username + password). Every new user gets a personal workspace.
- **`/login`** / **`/logout`** — user session.
- **`/account`** — your CLI devices, your recent audit events, and a revoke control (session-authenticated, no bearer token handed to the browser).
- **`/workspaces`** — list and create workspaces, and manage members: add by username, change roles (`owner`/`admin`/`editor`/`viewer`), remove, and view quotas. Role checks are enforced server-side on every action.

A CLI device is tied to the account signed in at `/cli` when it approves the device, so tokens minted from it resolve to that user's ownership.

## Page thumbnails (optional)

The dashboard can show a screenshot thumbnail per page. It's **off by default** and only exists in a build with the `thumbnails` Cargo feature on `pipa-server`; see [development](development.md) and [architecture](architecture.md) for how it works and its config.
