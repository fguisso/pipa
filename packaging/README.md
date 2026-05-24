# packaging/

Build artifacts, install scripts, and OS-level integration for `pipa-server`.
Everything here is downstream of the binary — nothing in this directory is
linked into the Rust build.

## What's in here

| Path                              | Purpose                                                                 |
|-----------------------------------|-------------------------------------------------------------------------|
| `install.sh`                      | One-shot installer for Linux. Downloads a release, verifies SHA256, drops the systemd unit + default config, prints next steps. |
| `systemd/pipa-server.service`    | Hardened systemd unit. Runs as the `pipa` user with `NoNewPrivileges`, `ProtectSystem=strict`, and friends. |

## Conventions

- The binary is installed to `/usr/local/bin/pipa-server` by default.
  Override with `PIPA_PREFIX`.
- Config lives at `/etc/pipa/pages.toml` (mode `0640`, owner `root:pipa`).
- Mutable state (SQLite, pages, key material) lives at `/var/lib/pipa/`
  (owner `pipa:pipa`, mode `0750`).
- The service user is `pipa` / group `pipa`, system account, no login
  shell, no home directory of its own besides `/var/lib/pipa`.
- TLS is **always** external. The unit binds HTTP on `127.0.0.1:8080` and
  expects Caddy (or another reverse proxy) in front of it. See
  `../Caddyfile.example` and `../.claude/project-docs/adr/0005-tls-termination.md`.

## Future layout

The intent is for this directory to host packaging metadata for the main
distros and package managers as they land:

```
packaging/
├── install.sh                  # done
├── systemd/                    # done
├── debian/                     # planned — debhelper rules, .deb postinst
├── rpm/                        # planned — .spec file for Fedora / openSUSE
├── homebrew/                   # planned — formula for macOS dev installs
└── nix/                        # planned — flake + NixOS module
```

When adding a new packaging target, prefer to call into `install.sh`'s logic
(or factor shared bits into a helper) rather than re-implement user/dir
creation per packager.

## Releasing (rough sketch)

The install script downloads from
`https://github.com/fguisso/pipa/releases/download/<TAG>/pipa-server-<TAG>-<TRIPLE>.tar.gz`
with a sibling `.sha256` file. The release pipeline (TBD, lives in
`.github/workflows/`) must:

1. Build `pipa-server` for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
2. `tar -czf pipa-server-<TAG>-<TRIPLE>.tar.gz pipa-server`.
3. `sha256sum pipa-server-<TAG>-<TRIPLE>.tar.gz > pipa-server-<TAG>-<TRIPLE>.tar.gz.sha256`.
4. Upload both as release assets.
