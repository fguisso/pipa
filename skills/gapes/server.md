# gapes — server install (agent-agnostic)

The client skill (`SKILL.md`) assumes a running server. This is the one-time
setup of a `gapes-server` instance. Ask the human for anything environment-
specific (domain, reverse-proxy details, which zones).

## 1. Get the binary

- Prebuilt: download `gapes-server-<target>` from the GitHub release (same place
  the CLI installer pulls from), or
- From source: `cargo build --release -p gapes-server` (add
  `--features gapes-server/zone` to enforce the `zone` axis — see below). See
  also `packaging/` for an install script + systemd unit.

## 2. Configure `pages.toml`

Start from `pages.example.toml`. Key sections:

- `[server]` — `addr` (bind, HTTP-only by design — put a reverse proxy in
  front), `public_url`, `data_dir`, `pages_dir`, `trusted_proxy` (the proxy IP
  whose `X-Forwarded-For` you trust).
- `[zone]` — **only honored if the binary was built with the `zone` feature.**
  ```toml
  [zone]
  default = "private"                  # zone new deploys land in (secure default)
  internal_proxy_ips = ["10.0.0.10"]   # reverse-proxy IP(s) = the internal channel
  internal_hosts = ["*.internal.example"]  # Host(s) that count as internal
  ```
  A request is the internal (LAN) zone only when its proxy peer IP **and** Host
  both match — so an internet visitor can't forge their way to a private page.

Do not commit a real `pages.toml`; it's environment config.

## 3. Reverse proxy + zones

`gapes-server` is HTTP-only on purpose. Put nginx / Caddy / a tunnel in front
for TLS. If you use the `zone` feature, the two channels are distinguished by
the proxy peer IP + Host:

- **internal** front door (LAN) → set its IP/host in `[zone]`.
- **external** front door (internet / tunnel) → anything not matching internal.

## 4. Run it

- Via systemd (see `packaging/systemd`): set `WorkingDirectory` to the install
  dir, `ExecStart` to the `gapes-server` binary, enable + start.
- First boot prints: open `<public_url>/setup` in a browser and create the admin
  (no pre-shared code — it's a wizard). Then a human can run the client login.

## 5. Verify

```sh
curl -fsS <server>/health        # 200
# (authenticated, from a logged-in client:)
gapes server --json              # {"features":[...]} — confirms `zone` etc. are enforced
```
