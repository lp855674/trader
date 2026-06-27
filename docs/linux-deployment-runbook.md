# Linux Deployment Runbook

This runbook covers the current release package for `trader-server` and the companion `trader` CLI.

## Release contents

Each Linux release archive contains:

- `trader-server`: REST/WebSocket service binary
- `trader`: CLI binary for migrate/backtest/paper/replay/ops tasks
- `linux-service.sh`: systemd install/update helper
- `configs/trader-server.example.toml`: example config file
- `VERSION`, `README.md`, `api.md`

## Runtime configuration

`trader-server` currently reads these environment variables:

- `TRADER_CONFIG`
  Path to the TOML config file that the server loads at startup.
- `TRADER_DATABASE_URL`
  Optional override for `[database].url` inside the config file.
  Example: `sqlite:///var/lib/trader/trader.sqlite`
- `TRADER_SERVER_BIND`
  HTTP bind address.
  Default: `127.0.0.1:8080`
- `RUST_LOG`
  Rust tracing filter.
  Example: `info` or `trader_server=debug,api=debug`

The Windows helper scripts in the repo mirror these same variable names so local runs and Linux service runs stay aligned.

## Install from release archive

1. Download and extract the release archive on the target host.
2. Review `configs/trader-server.example.toml`.
3. Install the binaries into `/opt/trader` and the service unit:

```bash
sudo ./linux-service.sh install
```

The install command creates:

- `/opt/trader`
- `/var/lib/trader`
- `/etc/trader/config.toml`
- `/etc/trader/trader-server.env`
- `/etc/systemd/system/trader-server.service`

If `/etc/trader/config.toml` does not exist yet, the script copies the packaged example config there.

## Required files to review before start

Edit `/etc/trader/trader-server.env`:

```dotenv
TRADER_CONFIG=/etc/trader/config.toml
TRADER_DATABASE_URL=sqlite:///var/lib/trader/trader.sqlite
TRADER_SERVER_BIND=127.0.0.1:8080
RUST_LOG=info
```

Edit `/etc/trader/config.toml`:

- Update `[data]` paths to real local datasets if you will run backtest/paper/replay jobs through API or CLI.
- Update broker/runtime settings before using non-simulated flows.
- Keep `[database].url` consistent with `TRADER_DATABASE_URL`, or rely on the env override only.

## Start and verify

```bash
sudo systemctl start trader-server
sudo systemctl status trader-server
curl http://127.0.0.1:8080/api/v1/health
```

Follow logs:

```bash
sudo ./linux-service.sh logs
```

## Update and rollback

Check the latest GitHub release:

```bash
sudo ./linux-service.sh check
```

Update to latest:

```bash
sudo ./linux-service.sh update
```

Update to a specific tag:

```bash
sudo ./linux-service.sh update --version v0.1.0
```

Rollback to the newest local backup:

```bash
sudo ./linux-service.sh rollback
```

## CLI usage on the server

The release also installs `/opt/trader/trader`. Typical examples:

```bash
/opt/trader/trader migrate --config /etc/trader/config.toml
/opt/trader/trader check-config --config /etc/trader/config.toml
/opt/trader/trader logs metrics --config /etc/trader/config.toml
```

## Network exposure

For initial deployment, prefer:

- `TRADER_SERVER_BIND=127.0.0.1:8080`
- expose it through nginx, Caddy, or another reverse proxy
- restrict access with host firewall / security group rules

If you bind to `0.0.0.0:8080`, do it intentionally and protect the host.
