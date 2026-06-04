# Production deploy

## Option A: Docker compose (easiest)

```sh
cd docker/
cp ../pages.example.toml ./pages.toml
$EDITOR ./Caddyfile          # set your domain + Let's Encrypt email
docker compose up -d
```

Multi-arch image (`amd64` / `arm64`) built from [`docker/Dockerfile`](../docker/Dockerfile). Only Caddy is exposed on the host (`:80`, `:443`); `pipa-server` stays on the internal bridge. State persists in `docker/data/` and `docker/pages/`.

## Option B: install script + systemd + Caddy

```sh
curl -fsSL https://raw.githubusercontent.com/fguisso/pipa/main/packaging/install.sh \
  | sudo bash
```

The script downloads the right binary for your arch, verifies its SHA-256, creates a `pipa` system user, drops `/etc/pipa/pages.toml`, and installs a hardened systemd unit. Pin a release with `sudo PIPA_VERSION=v0.1.0 bash install.sh`.

Then put Caddy in front:

```sh
sudo apt install caddy
sudo cp Caddyfile.example /etc/caddy/Caddyfile
sudo $EDITOR /etc/caddy/Caddyfile      # domain + Let's Encrypt email
sudo $EDITOR /etc/pipa/pages.toml     # public_url, trusted_proxy
sudo systemctl reload caddy
sudo systemctl enable --now pipa-server
```

## Option C: build from source

```sh
cargo build --release -p pipa-server
sudo install -m 755 target/release/pipa-server /usr/local/bin/
sudo install -m 644 packaging/systemd/pipa-server.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now pipa-server
```
