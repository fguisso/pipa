# Security

- TLS is the proxy's job. `pipa-server` listens HTTP only and refuses non-loopback in `--dev`.
- Default response headers: strict CSP on hosted pages, `nosniff`, conservative `Permissions-Policy`, no `Server` header. The default CSP can be opted out per-page (`--csp off` on deploy/share) for sites that legitimately load CDN assets; those pages then declare their own policy via `<meta http-equiv>`.
- Passwords: argon2id (64 MiB, t=3, p=1). Tokens: HMAC, hashed at rest, rotated on every use.
- Uploads: path-traversal blocked, symlinks rejected, exec bit dropped, 100 MB cap, zip-bomb guarded.
- Destructive ops always require a fresh second-device confirmation (QR or URL).
- Audit log is append-only; every authenticated mutation lands a row.
- **Ownership isolation.** A user only reaches its own pages; workspace access is role-gated (`viewer` reads, `editor`+ writes) and enforced server-side on every page endpoint (403 `not_owner` / `insufficient_role`). The `local` operator is the only superuser.
- **Credential sources, no silent downgrade.** `PIPA_REFRESH_TOKEN` → external command vault (`PIPA_SECRET_GET_CMD`/`SET_CMD`, e.g. 1Password/Bitwarden) → OS keychain → `pass` → age file → chmod-600 file. `--headless` restricts this to the command vault and env token only — it never touches the keychain and never falls back to a file, failing loudly instead of hanging.
- **Thumbnail capture is isolated.** The optional `thumbnails` feature screenshots pages via a short-lived loopback-only (`127.0.0.1`) static server over the on-disk bundle — the public serve path is untouched and no gate-bypass token exists. Thumbnails are stored outside the bundle and served admin-only.

This page covers the always-on defaults and the threat model.
