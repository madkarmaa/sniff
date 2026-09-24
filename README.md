<h1 align="center">
  <img src="./.github/logo.gif" alt="sniffer from minecraft" width="320">
</h1>

**Sniff** is a specialized API service designed to retrieve Google Play Store app
details across different release channels (Stable, Beta, Alpha). It provides a clean
interface to access app metadata including version information, changelog, download
sizes, and other details for Android applications.

## Features

- **Multi-Channel Support**: Access app details from Stable, Beta, and Alpha channels (where available)
- **Intelligent Track Detection**: Automatically identifies which channels are available for specific apps
- **Multi-Arch Downloads**: Download info merges splits across devices (arm64-v8a, armeabi-v7a, plus density/locale) so one call lists every APK split the app ships
- **Unified API**: Simple REST API endpoints for accessing app information

## API Endpoints

Interactive API reference is served at `/docs`.

### Get App Details (All Available Channels)

```
GET /v1/details/:package_name
```

Returns details for all available channels for the specified package.

**Response Headers:**

- `X-Available-Channels`: Comma-separated list of available channels for the app

Successful response:

```jsonc
{
    "success": true,
    "data": {
        // channel name -> details (only available channels are present)
        "stable": { "item": {/* see single-channel shape below */} },
        "beta": { "item": {/* ... */} }
        // "alpha" if available
    },
    "error": null
}
```

### Get App Details (Specific Channel)

```
GET /v1/details/:package_name/:channel
```

Returns details for a specific channel (stable, beta, or alpha) if available.

**Possible channels:**

- `stable` - Production release (always available)
- `beta` - Beta program release (only available for certain apps)
- `alpha` - Alpha program release (only available for certain apps)

**Response Format:**

Successful response (`GET /v1/details/com.discord/stable`, trimmed):

```json
{
    "success": true,
    "data": {
        "item": {
            "id": "com.discord",
            "sub_id": "com.discord",
            "type": 1,
            "category_id": 3,
            "title": "Discord - Talk, Play, Hang Out",
            "creator": "Discord Inc.",
            "description_html": "Discord is designed for gaming...",
            "offer": [
                {
                    "micros": 0,
                    "currency_code": "EUR",
                    "formatted_amount": "",
                    "checkout_flow_required": false,
                    "offer_type": 1
                }
            ],
            "details": {
                "app_details": {
                    "developer_name": "Discord Inc.",
                    "version_code": 345009,
                    "version_string": "345.9 - Stable",
                    "info_download_size": 159659592,
                    "developer_email": "support@discord.com",
                    "developer_website": "https://dis.gd/contact",
                    "info_download": "500,000,000+ downloads",
                    "package_name": "com.discord",
                    "recent_changes_html": "We've been hard at work...",
                    "info_updated_on": "Sep 11, 2026",
                    "target_sdk_version": 36
                }
            },
            "app_info": {
                "section": [
                    {
                        "label": "In-app purchases",
                        "container": {
                            "description": "€0.84 - €274.99 if billed through Play"
                        }
                    },
                    {
                        "label": "Offered by",
                        "container": { "description": "Google Commerce Ltd" }
                    },
                    {
                        "label": "Released on",
                        "container": { "description": "May 13, 2015" }
                    }
                ]
            },
            "mature": false,
            "promotional_description": "Group Chat That's Fun & Games",
            "available_for_preregistration": false,
            "force_shareability": false
        },
        "footer_html": "All prices include VAT.",
        "enable_reviews": false
    },
    "error": null
}
```

Error responses:

```json
{
    "success": false,
    "data": null,
    "error": "Error message describing the issue"
}
```

### Get Download Info for a Specific App Version

```
GET /v1/download/:package_name/:channel/:version_code
```

Retrieves the requested version, converts `base.apk` and every returned split into
reversible BMP parts, and uploads them to Google Photos using that channel's
Google Play account. The request waits for uploads and a durable D1 history record.
Metadata-only `/v1/details` requests do not upload anything.

`data.photos` contains each APK's size, SHA-256, and ordered Photos media keys.
Completed versions are served from history without contacting Play or uploading
again; their expiring Play URL fields are empty. `/v1/history/:package_name/:channel`
lists the latest 100 requested archives for the currently configured channel account.
See [ARCHIVING.md](ARCHIVING.md) for recovery, reconstruction, and deployment setup.
Splits are merged across download devices (`px_9a`/`arm64-v8a`,
`sm_a13_5g`/`armeabi-v7a`, `google_kiwi_x86_64`/`x86`+`x86_64`), so the response
lists every architecture the app actually ships. ABIs the app has no build for
(e.g. x86 for arm-only apps) are skipped — a partial merge still returns 200.

**Parameters:**

- `package_name`: The package identifier of the app (e.g., `com.discord`)
- `channel`: Release channel (`stable`, `beta`, or `alpha`)
- `version_code`: The specific Android version code to download (integer, see `details.app_details.version_code`)

**Response Format:**

Successful responses include the existing Play URL fields and a `photos` array.
Each entry records the APK name, byte length, SHA-256, completion state, and ordered
BMP parts with their Photos media keys and SHA-1 hashes. [Example manifest](ARCHIVING.md#history-and-response).

Archive failures return HTTP 502 with `success: false` and the partial manifest in
`data.photos`; confirmed uploads are retained in history. Failures before archive
creation return the standard error envelope below.

Error response:

```json
{
    "success": false,
    "data": null,
    "error": "App not found or version unavailable"
}
```

## Build From Source

Prerequisites:

- Rust stable with the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- [Bun](https://bun.sh/) (used to run Wrangler)
- A Cloudflare account for deploy only (not needed for local dev)

```bash
git clone https://github.com/madkarmaa/sniff
cd sniff
rustup target add wasm32-unknown-unknown
```

### 1. Mint Google Play tokens

Each channel needs a Google account email plus an AAS token. To mint an AAS token
from a one-time OAuth token (see `gpapi/src/lib.rs` docs for how to grab the
`oauth2_4/` cookie), use the helper:

```bash
cargo run -p oauth2aas -- <email> <oauth_token>
```

It prints the AAS token. Repeat for every account you plan to use.

### 2. Local dev

Create `.dev.vars` (gitignored, picked up automatically by `wrangler dev`):

```
DEVICE_NAME="sm_a13_5g"
STABLE_EMAIL="..."
STABLE_AAS_TOKEN="..."
BETA_EMAIL="..."
BETA_AAS_TOKEN="..."
ALPHA_EMAIL="..."
ALPHA_AAS_TOKEN="..."
```

`STABLE_*` is required; beta/alpha pairs are optional and only needed for those
channels. Then:

```bash
bun install
bunx wrangler d1 migrations apply sniff-history --local
bun run dev
curl -fsSL -A "Mozilla/5.0" http://localhost:8787/v1/details/com.discord/stable
```

### 3. Checks

```bash
cargo check --all
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bun run test:worker
```

## Environment Variables

The following environment variables are required:

- `DEVICE_NAME`: Device identifier for Google Play API (default `sm_a13_5g` in `wrangler.toml`)
- `STABLE_EMAIL`: Email for stable track access
- `STABLE_AAS_TOKEN`: Authentication token for stable track
- `BETA_EMAIL`: Email enrolled in beta programs
- `BETA_AAS_TOKEN`: Authentication token for beta access
- `ALPHA_EMAIL`: Email enrolled in alpha programs
- `ALPHA_AAS_TOKEN`: Authentication token for alpha access

## Native Google Photos uploader

The API uses the native Photos library in
[`uploader/`](uploader/README.md) and the reversible file-to-BMP `converter`. Both also have standalone CLIs.
The uploader accepts `STABLE_EMAIL`/`STABLE_AAS_TOKEN` at runtime and supports batch
upload, hash checks and verified original downloads. See [RESEARCH.md](RESEARCH.md)
for the gotohp reference and live JPEG/BMP round-trip proof.
