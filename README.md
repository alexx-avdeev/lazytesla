# LazyTesla

A terminal UI for managing your Tesla via the [Tesla Fleet API](https://developer.tesla.com/docs/fleet-api). Built with Rust and [ratatui](https://ratatui.rs/).

## Features

- OAuth sign-in with token persistence (session restore on restart)
- Two-panel vehicle view: list on the left, cached details on the right
- Vehicle details fetched on startup and on manual refresh
- Climate on/off toggle and target temperature setting (native Vehicle Command Protocol or optional HTTP proxy)
- Charge limit setting (% SOC)
- Window vent/close toggle
- Start/stop charging (Alt+C; also listed in the `?` help palette)
- Help palette (`?`) listing all hotkeys with ↑/↓ + Enter to run
- Lock/unlock toggle (native VCP via VCSEC or optional HTTP proxy)
- Masked VIN display
- Optional Fleet API debug logging to a local file

## Prerequisites

- **Rust** (2024 edition; nightly or recent stable)
- A **Tesla developer account** and Fleet API application at [developer.tesla.com](https://developer.tesla.com)
- **Go** (optional — only if you prefer Tesla's [vehicle-command](https://github.com/teslamotors/vehicle-command) HTTP proxy over native signing)

## Tesla developer setup

1. Create an application on [developer.tesla.com](https://developer.tesla.com) and note the **Client ID** and **Client Secret**.
2. Add redirect URI: `http://localhost:8484/callback` (or your custom value — must match `TESLA_REDIRECT_URI`).
3. Enable scopes: `openid`, `offline_access`, `user_data`, `vehicle_device_data`, `vehicle_cmds`, `vehicle_charging_cmds`.
4. Register a **domain** for your app (e.g. `example.com`).
5. Generate a command-authentication key pair and host the public key:

   ```bash
   # Build tesla-keygen from source (see "Vehicle Command Proxy" below)
   tesla-keygen create > public_key.pem
   ```

   Host `public_key.pem` at:

   ```
   https://<your-domain>/.well-known/appspecific/com.tesla.3p.public-key.pem
   ```

6. **Pair your app** on the vehicle: open `https://tesla.com/_ak/<your-domain>` in the Tesla mobile app (v4.27.3+). The vehicle must be online.

## Environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TESLA_CLIENT_ID` | yes | — | OAuth client ID from developer.tesla.com |
| `TESLA_CLIENT_SECRET` | yes | — | OAuth client secret |
| `TESLA_DOMAIN` | yes* | — | Registered app domain (needed for partner registration) |
| `TESLA_REDIRECT_URI` | no | `http://localhost:8484/callback` | OAuth redirect URI |
| `TESLA_AUDIENCE` | no | `https://fleet-api.prd.na.vn.cloud.tesla.com` | Fleet API region base URL |
| `TESLA_CALLBACK_PORT` | no | `8484` | Local port for OAuth callback server |
| `TESLA_FLEET_KEY` | no** | — | Path to fleet private key PEM (`tesla-keygen create`); preferred for climate/lock/commands |
| `TESLA_KEY_FILE` | no | — | Alias for `TESLA_FLEET_KEY` |
| `TESLA_COMMAND_PROXY_URL` | no*** | — | Vehicle Command Proxy URL (fallback; use `https://127.0.0.1:4443`, not `localhost`) |
| `TESLA_COMMAND_PROXY_CA_CERT` | no*** | — | Path to the proxy TLS certificate (`tls-cert.pem`) |
| `TESLA_DEBUG_CURL` | no | — | Set to `1` to log equivalent `curl` commands to a file |
| `TESLA_DEBUG_CURL_LOG` | no | see below | Override path for the debug log file |

\*Required for vehicle list/data in most regions. Without it you'll see a registration error on refresh.

\**Required for climate (`c`) and lock (`u`) on modern vehicles unless you use the proxy below. Pre-2021 Model S/X may work without signed commands for some actions.

\***Proxy fallback when `TESLA_FLEET_KEY` is not set. If both are set, native VCP takes priority.

### Example `.env` snippet

```bash
export TESLA_CLIENT_ID="your-client-id"
export TESLA_CLIENT_SECRET="your-client-secret"
export TESLA_DOMAIN="example.com"
```

## Build and run

```bash
cargo build --release
cargo run
```

On first launch, press **Enter** to sign in. Your browser opens for Tesla OAuth; after approval you're redirected to `localhost:8484` and returned to the TUI.

Tokens are stored at:

```
~/Library/Application Support/lazytesla/tokens.json   # macOS
~/.config/lazytesla/tokens.json                       # Linux
```

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Sign in (auth screen) |
| `↑` / `k` | Previous vehicle |
| `↓` / `j` | Next vehicle |
| `r` | Refresh vehicle list and details |
| `c` | Toggle climate on/off (selected vehicle) |
| `Alt+c` | Start or stop charging (selected vehicle; not shown in footer) |
| `t` | Set target AC temperature (uses vehicle display units: °F or °C) |
| `b` | Set charge limit (% SOC) |
| `+` / `-` / `↑` / `↓` | Adjust value while editing temp or charge limit |
| `u` | Toggle lock/unlock (selected vehicle) |
| `w` | Toggle windows vent/close (selected vehicle) |
| `?` | Open help palette (↑/↓ move, Enter run, Esc close) |
| `l` | Log out |
| `q` | Quit |

Vehicle details are cached in memory. Switching vehicles shows cached data immediately; press `r` to fetch fresh data from the API.

## Vehicle commands (climate and lock)

Modern Teslas require commands to be signed via Tesla's [Vehicle Command Protocol](https://github.com/teslamotors/vehicle-command). LazyTesla signs commands natively in Rust when `TESLA_FLEET_KEY` is set, posting directly to Fleet API `signed_command`. Climate uses the infotainment domain; lock/unlock uses VCSEC (vehicle security). Session state is cached at `~/Library/Application Support/lazytesla/session_cache.json` (macOS).

### Native signing (recommended)

```bash
# After tesla-keygen create (see developer setup above)
export TESLA_FLEET_KEY="/path/to/config/fleet-key.pem"
```

No proxy process required. Pair your key on the vehicle via `https://tesla.com/_ak/<your_domain>`.

## Vehicle Command Proxy (optional fallback)

If you prefer Tesla's official HTTP proxy instead of native signing, LazyTesla can still route commands through it when `TESLA_FLEET_KEY` is unset.

### 1. Install the proxy

`go install ...@latest` does not work for this repo (its `go.mod` has `replace` directives). Clone and build locally:

```bash
git clone https://github.com/teslamotors/vehicle-command.git
cd vehicle-command
go build -o ~/go/bin/tesla-http-proxy ./cmd/tesla-http-proxy
go build -o ~/go/bin/tesla-keygen ./cmd/tesla-keygen
```

Add Go binaries to your `PATH` (once):

```bash
export PATH="$HOME/go/bin:$PATH"
```

Add that line to `~/.zshrc` if you want it in every new terminal.

Keep your fleet private key (e.g. `config/fleet-key.pem`) from the `tesla-keygen create` step.

### 2. Create a TLS certificate for the proxy

```bash
mkdir -p config
openssl req -x509 -nodes -newkey ec \
  -pkeyopt ec_paramgen_curve:secp384r1 \
  -subj '/CN=localhost' \
  -keyout config/tls-key.pem -out config/tls-cert.pem -sha256 -days 3650 \
  -addext 'subjectAltName=DNS:localhost,IP:127.0.0.1' \
  -addext 'basicConstraints=CA:FALSE' \
  -addext 'extendedKeyUsage=serverAuth'
```

Use a **server** certificate (`CA:FALSE`), not OpenSSL's default CA cert. The SAN entries let clients verify `https://127.0.0.1:4443` without disabling TLS checks.

### 3. Run the proxy (separate terminal)

```bash
tesla-http-proxy \
  -tls-key config/tls-key.pem \
  -cert config/tls-cert.pem \
  -key-file config/fleet-key.pem \
  -port 4443
```

### 4. Configure LazyTesla

```bash
export TESLA_COMMAND_PROXY_URL=https://127.0.0.1:4443
# Required — use an absolute path; the proxy uses a self-signed TLS cert
export TESLA_COMMAND_PROXY_CA_CERT="/Users/you/Development/Learning/lazytesla/config/tls-cert.pem"
```

`TESLA_COMMAND_PROXY_CA_CERT` is required when the proxy URL is set. LazyTesla adds that PEM as the only trusted root for the proxy client — full certificate verification, no TLS bypass.

**Test the proxy** (a `403` with "client did not provide an OAuth token" means TLS is working):

```bash
curl -s --cacert config/tls-cert.pem https://127.0.0.1:4443/api/1/vehicles
```

Then run LazyTesla and use **`c`** to toggle climate.

## Debug logging

To log Fleet API requests as `curl` commands (useful for debugging):

```bash
export TESLA_DEBUG_CURL=1
```

Logs append to:

```
~/Library/Application Support/lazytesla/fleet-api.log   # macOS
```

Override with `TESLA_DEBUG_CURL_LOG=/path/to/log`. Debug output goes to a file only — the TUI is not affected.

## Tests

```bash
cargo test
```

- **Unit tests** in `src/` (parsing, app state, auth helpers)
- **Integration tests** in `tests/fleet_api.rs` (HTTP mocks via wiremock)

## Project layout

```
src/
  main.rs          # TUI event loop
  app.rs           # Application state
  config.rs        # Environment configuration
  api/             # Fleet API client, vehicle data, commands
  auth/            # OAuth, token store, callback server
  vehicle_command/ # Native Tesla Vehicle Command Protocol (signed_command)
  tui/             # ratatui screens
tests/
  fleet_api.rs     # Integration tests
  vehicle_command.rs # VCP signing and Fleet API wiremock tests
```

## Regions

Default audience is North America (`fleet-api.prd.na.vn.cloud.tesla.com`). For other regions, set `TESLA_AUDIENCE` to the appropriate Fleet API URL from [Tesla's regional documentation](https://developer.tesla.com/docs/fleet-api/getting-started/regions-countries).
