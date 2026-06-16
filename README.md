# torizon-cloud-cli

Unofficial command-line interface for the [Torizon Cloud](https://www.toradex.com/torizon)
(Torizon OTA v2) API. It wraps the REST API at `https://app.torizon.io/api/v2`
with OAuth2 `client_credentials` authentication.

> This is a third-party tool and is not affiliated with or supported by Toradex.

## Install

```bash
cargo install --path .
# installs the `torizon` binary into ~/.cargo/bin

# or just build:
cargo build --release   # ./target/release/torizon  (or $CARGO_TARGET_DIR/release/torizon)
```

### Shell completions

```bash
torizon completions bash > /etc/bash_completion.d/torizon   # bash
torizon completions zsh  > "${fpath[1]}/_torizon"           # zsh
torizon completions fish > ~/.config/fish/completions/torizon.fish
```

## Authentication

Create an **API client** in the Torizon Cloud web UI to obtain a *client ID*
and *client secret*, then log in:

```bash
torizon login --client-id <ID> --client-secret <SECRET>
# or run `torizon login` and enter them interactively
```

Credentials are stored in `~/.config/torizon/credentials.toml` (mode `0600` on
Unix). Access tokens are cached in `~/.config/torizon/token-cache.json` and
refreshed automatically.

You can keep several named profiles:

```bash
torizon login --profile staging --client-id ... --client-secret ...
torizon --profile staging devices list
```

## Commands

```bash
# Devices
torizon devices list [--limit N] [--offset N] [--name-contains STR] [--tag T]... [--hibernated true|false]
torizon devices get|assignment|components|packages <DEVICE_UUID>
torizon devices network [<DEVICE_UUID>]
torizon devices name  <DEVICE_UUID> [--set "New name"]
torizon devices notes <DEVICE_UUID> [--set "..."]
torizon devices tags  <DEVICE_UUID>
torizon devices set-tags <DEVICE_UUID> --tag key=value [--tag ...]
torizon devices hibernate <DEVICE_UUID> --on|--off
torizon devices create --device-id ID [--name NAME] [--fleet FID]... [--tag k=v]...
torizon devices delete <DEVICE_UUID> [-y]
torizon devices token

# Packages
torizon packages list [--name-contains STR] [--id-contains STR] [--version V] [--hardware-id HW]... \
                      [--sort-by filename|created-at] [--sort-direction asc|desc] [--limit N] [--offset N]
torizon packages get <PACKAGE_ID>
torizon packages upload --name NAME --version VER --hardware-id HW [--hardware-id HW2]... --format OSTREE --file ./pkg
torizon packages edit <PACKAGE_ID> [--comment "..."] [--hardware-id HW]...
torizon packages delete <PACKAGE_ID> [-y]
torizon packages external

# Updates
torizon updates launch --package <PKG_ID> [--package ...] (--device <UUID>... | --fleet <FLEET_ID>...)
torizon updates cancel <UPDATE_ID>
torizon updates list   <DEVICE_UUID>          # full update history (status + result code)
torizon updates status <DEVICE_UUID>          # most recent update
torizon updates watch  <DEVICE_UUID> [--interval 30] [--timeout 3600]   # poll until done/failed

# Fleets
torizon fleets list
torizon fleets get <FLEET_ID>
torizon fleets create --name NAME --type static
torizon fleets create --name NAME --type dynamic --expression "<filter>"
torizon fleets delete <FLEET_ID> [-y]
torizon fleets devices <FLEET_ID>
torizon fleets add-devices <FLEET_ID> --device <UUID> [--device ...]
torizon fleets remove-devices <FLEET_ID> --device <UUID> [--device ...] [-y]

# Metrics (from/to are UNIX epoch seconds)
torizon metrics names
torizon metrics device|detailed <DEVICE_UUID> --from <EPOCH> --to <EPOCH> [--metric NAME]... [--resolution SECS]
torizon metrics fleet|outliers|report <FLEET_ID> --from <EPOCH> --to <EPOCH> [--metric NAME]...

# Lockboxes (offline updates)
torizon lockboxes list | details
torizon lockboxes get <NAME>
torizon lockboxes set <NAME> --package <PKG_ID> [--package ...] [--expires-at 2026-12-31T00:00:00Z]
torizon lockboxes delete <NAME> [-y]

# Remote access
torizon remote-access device|sessions <DEVICE_UUID>
torizon remote-access create-session <DEVICE_UUID> --public-key "ssh-..." [--duration 30m]
torizon remote-access delete-session <DEVICE_UUID> [-y]
torizon remote-access user-sessions | keys | ip-list
torizon remote-access add-key --pubkey "ssh-..."   |   remove-key <KEY_ID>
torizon remote-access add-ip <IP>                  |   remove-ip <IP>
```

The HTTP client automatically honours rate limiting (HTTP 420 with `Retry-After`).

Add `--json` to any command for raw JSON output instead of tables. In `--json`
mode, status messages are suppressed so the output is always machine-parseable.

## Configuration file format

`~/.config/torizon/credentials.toml`:

```toml
default = "default"

[profiles.default]
client_id = "..."
client_secret = "..."
# optional overrides:
# api_base = "https://app.torizon.io/api/v2"
# token_url = "https://kc.torizon.io/auth/realms/ota-users/protocol/openid-connect/token"
```
