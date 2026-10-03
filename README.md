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
- **Unified API**: One endpoint returns app details and download URLs for the latest version

## API Endpoints

Interactive API reference is served at `/v2/docs`; the OpenAPI document is
available at `/v2/openapi.json`.

### Get Latest App Details and Download Info

```
GET /v2/download/:package_name/:channel
```

Returns app metadata and download URLs for the latest version available on the
requested channel. Google Play only lists the latest version, so no build ID or
version code is required in the URL. Metadata and download fields are merged
under `data`; the selected version is in `data.item.details.app_details.version_code`.
All download targets use that version.

Splits are merged across download devices (`px_9a`/`arm64-v8a`,
`sm_a13_5g`/`armeabi-v7a`, `google_kiwi_x86_64`/`x86`+`x86_64`). ABIs whose
requests fail are skipped; a partial merge still returns 200.

**Parameters:**

- `package_name`: The package identifier of the app (e.g., `com.discord`)
- `channel`: Release channel (`stable`, `beta`, or `alpha`)

**Possible channels:**

- `stable` - Production release (always available)
- `beta` - Beta program release (only available for certain apps)
- `alpha` - Alpha program release (only available for certain apps)

**Response Format:**

Successful response (`GET /v2/download/com.discord/stable`, trimmed):

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
                        "container": {
                            "description": "Google Commerce Ltd"
                        }
                    },
                    {
                        "label": "Released on",
                        "container": {
                            "description": "May 13, 2015"
                        }
                    }
                ]
            },
            "mature": false,
            "promotional_description": "Group Chat That's Fun & Games",
            "available_for_preregistration": false,
            "force_shareability": false
        },
        "footer_html": "All prices include VAT.",
        "enable_reviews": false,
        "main_apk_url": "https://play.googleapis.com/download/by-token/download?token=...",
        "splits": [
            {
                "name": "config.arm64_v8a",
                "download_url": "https://play.googleapis.com/download/by-token/download?token=..."
            },
            {
                "name": "config.en",
                "download_url": "https://play.googleapis.com/download/by-token/download?token=..."
            },
            {
                "name": "config.it",
                "download_url": "https://play.googleapis.com/download/by-token/download?token=..."
            },
            {
                "name": "config.xxhdpi",
                "download_url": "https://play.googleapis.com/download/by-token/download?token=..."
            },
            {
                "name": "config.armeabi_v7a",
                "download_url": "https://play.googleapis.com/download/by-token/download?token=..."
            },
            {
                "name": "config.xhdpi",
                "download_url": "https://play.googleapis.com/download/by-token/download?token=..."
            }
        ],
        "additional_files": [],
        "dex_metadata_url": "https://play.googleapis.com/download/by-token/download?token=..."
    },
    "error": null
}
```

Error response:

```json
{
    "success": false,
    "data": null,
    "error": "App 'com.discord' not found"
}
```

An invalid channel returns 400, an app not found returns 404, and upstream failures
return 500. Separate details endpoints are not part of v2; clients should use the
v2 download endpoint for both metadata and downloads. The `v1` branch continues
to serve the original v1 API for existing clients.

## Versioned Deployments

The `v2` branch is the default branch. The `v1` branch preserves the original API,
and `feat/archiving` remains a separate feature branch. Pushes to `v*` branches
trigger independent deployments; the workflow can also be run manually on a
version branch. Each branch deploys to a Worker named `sniff-<branch>` using
`sniff.madkarma.top/<branch>` and `sniff.madkarma.top/<branch>/*` routes.

- `v1` deploys `sniff-v1`, preserving all `/v1/*` API URLs.
- `v2` deploys `sniff-v2`, serving the new `/v2/*` API URLs.
- Versioned API references are at `https://sniff.madkarma.top/v1/docs` and
  `https://sniff.madkarma.top/v2/docs`.

These path routes take precedence over the existing `sniff` Worker's custom
domain, which remains the fallback for unversioned URLs. Both versioned Workers
receive their Google Play credentials from the same GitHub repository secrets
during deployment. Required secrets are `CLOUDFLARE_API_TOKEN`,
`CLOUDFLARE_ACCOUNT_ID`, `STABLE_EMAIL`, and `STABLE_AAS_TOKEN`; beta and alpha
credentials are optional. The Cloudflare token must be able to deploy Workers
and manage routes in the `madkarma.top` zone.

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
bun run dev
curl -fsSL -A "Mozilla/5.0" http://localhost:8787/v2/download/com.discord/stable
```

### 3. Checks

```bash
cargo check --all
cargo fmt --all -- --check
cargo clippy --all -- -D warnings
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
