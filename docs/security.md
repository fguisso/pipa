# Security

- TLS is the proxy's job. `pipa-server` listens HTTP only and refuses non-loopback in `--dev`.
- Default response headers: strict CSP on hosted pages, `nosniff`, conservative `Permissions-Policy`, no `Server` header. The default CSP can be opted out per-page (`--csp off` on deploy/share) for sites that legitimately load CDN assets; those pages then declare their own policy via `<meta http-equiv>`.
- Passwords: argon2id (64 MiB, t=3, p=1). Tokens: HMAC, hashed at rest, rotated on every use.
- Uploads: path-traversal blocked, symlinks rejected, exec bit dropped, 100 MB cap, zip-bomb guarded.
- Destructive ops always require a fresh second-device confirmation (QR or URL).
- Audit log is append-only; every authenticated mutation lands a row.

Read [`SECURITY.md`](../.claude/project-docs/SECURITY.md) for the always-on defaults and the threat model.
