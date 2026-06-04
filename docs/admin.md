# Admin dashboard

Browse to `http://127.0.0.1:8080/admin`. Paste a refresh token, get a 30-minute signed cookie, and see your pages, comments queue, devices, and audit log. It live-refreshes via Alpine.js and the same JSON API the CLI uses. No build step, no framework: `ui/public/vendor/alpine.min.js` is the only client-side dependency, served from the binary.

Destructive ops in the admin (delete page, flip to public, revoke another device) intentionally point you back at the CLI. They need the second-device step-up flow, which fits a terminal plus phone better than a single-tab browser.

Disable the admin entirely with `[admin] ui_enabled = false` in `pages.toml`.
