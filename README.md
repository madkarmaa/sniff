<h1 align="center">
  <img src="./.github/logo.gif" alt="sniffer from minecraft" width="320">
</h1>

Sniff is a Cloudflare Worker that returns Google Play app details and download URLs
for the stable, beta, and alpha release channels. The details include the version
code, changelog, and download size.

## Features

- One endpoint returns app details and download URLs for the latest version.
- Beta and alpha channels work for any app whose testing program the channel's account has joined.
- Splits are merged across arm64-v8a, armeabi-v7a, x86, and x86_64 devices, so one call lists every APK split the app ships.
- Every file comes with its size and hashes, and a `targets` list shows which ABIs made it into the merge.
- Successful lookups are cached at the edge for 120 seconds, and CORS is open so browser apps can call the API.

## API endpoints

Interactive API reference is served at `/v3/docs`; the OpenAPI document is
available at `/v3/openapi.json`.

### Get latest app details and download info

```
GET /v3/download/:package_name/:channel
```

Returns app metadata and download URLs for the latest version available on the
requested channel. Google Play only lists the latest version, so no build ID or
version code is required in the URL. Metadata and download fields are merged
under `data`; the selected version is in `data.item.details.app_details.version_code`.
All download targets use that version.

Splits are merged across download devices (`px_9a`/`arm64-v8a`,
`sm_a13_5g`/`armeabi-v7a`, `google_kiwi_x86_64`/`x86`+`x86_64`). ABIs whose
requests fail are left out of the merge, and a partial merge still returns 200.
`data.targets` lists every ABI with its device and whether it succeeded (plus a
short `error` when it did not), so a client can tell a complete merge from a partial
one. Only a lookup where every target failed is an error response.

Sizes are in bytes. `sha1` and `sha256` are passed through exactly as Google Play
reports them, typically base64url-style strings and not hex; decode them before
comparing against a locally computed digest. Any of these fields is `null` when
Google does not report it (expansion files never have a `sha256`). The size and
hashes next to a URL describe the file behind that URL.

**Parameters:**

- `package_name`: The package identifier of the app (e.g., `com.discord`): dot-separated
  segments of letters, digits and underscores, each starting with a letter, at most 255
  characters. Anything else is rejected with 400 before Google is contacted.
- `channel`: Release channel (`stable`, `beta`, or `alpha`, case-insensitive)

**Possible channels:**

- `stable` - Production release (always available)
- `beta` - Beta program release (when the `BETA_*` account has joined the app's testing program)
- `alpha` - Alpha program release (when the `ALPHA_*` account has joined the app's testing program)

**Response Format:**

Successful response (`GET /v3/download/com.discord/stable`, trimmed):

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
        "main_apk_size": 12345678,
        "main_apk_sha1": "yq3dhkoGu0Ub1SEcMRMyuP-g5Ps",
        "main_apk_sha256": "xT8jM2k0g5Y9bQpQzC1m0KqB1G5w2h3o4vZ7nL8aRdE",
        "splits": [
            {
                "name": "config.arm64_v8a",
                "download_url": "https://play.googleapis.com/download/by-token/download?token=...",
                "size": 4567890,
                "sha1": "aW1hZ2luYXJ5LXNoYTEtZGlnZXN0",
                "sha256": "bm90LWEtcmVhbC1zaGEyNTYtZGlnZXN0LWp1c3QtYW4tZXhhbXBsZQ"
            },
            {
                "name": "config.en",
                "download_url": "https://play.googleapis.com/download/by-token/download?token=...",
                "size": 102400,
                "sha1": "ZXhhbXBsZS1zcGxpdC1zaGEx",
                "sha256": "ZXhhbXBsZS1zcGxpdC1zaGEyNTYtZGlnZXN0LW5vdC1yZWFs"
            }
        ],
        "additional_files": [],
        "dex_metadata_url": "https://play.googleapis.com/download/by-token/download?token=...",
        "targets": [
            { "abi": "arm64-v8a", "device": "px_9a", "success": true, "error": null },
            { "abi": "armeabi-v7a", "device": "sm_a13_5g", "success": true, "error": null },
            { "abi": "x86", "device": "google_kiwi_x86_64", "success": false, "error": "Google Play returned HTTP 500" },
            { "abi": "x86_64", "device": "google_kiwi_x86_64", "success": true, "error": null }
        ]
    },
    "error": null
}
```

The sizes, hashes and the `targets` list are new in v3; the other fields are unchanged
from v2. In this example the `x86` target failed, so splits that only that target
would have contributed are missing. The hash values above are illustrative.

Error response:

```json
{
    "success": false,
    "data": null,
    "error": "App 'com.discord' not found"
}
```

**Status codes:**

| Status | Meaning |
| ------ | ------- |
| 200 | Lookup succeeded; check `data.targets` for partial merges |
| 400 | Invalid channel, invalid package name, or the channel's account has not joined the app's testing program |
| 404 | Google Play does not know the app |
| 422 | The app cannot be downloaded: Google refused the purchase or delivery for every target, typically a paid app or one not offered to the accounts and devices this service uses |
| 500 | Internal error or server misconfiguration (for example missing `STABLE_*` or `DEVICE_NAME`) |
| 501 | The requested channel's credentials are not configured on this deployment |
| 502 | Google Play failed: a non-2xx answer, network error, unusable response or login failure |
| 503 | Google Play is rate limiting this service; wait for the `Retry-After` delay (seconds) |

Error messages are short and never include upstream response bodies; the details go to
the Worker's logs.

**Headers and caching:**

- Every response carries `Access-Control-Allow-Origin: *`. `OPTIONS` requests under
  `/v3/` are answered with 204 and `Access-Control-Allow-Methods: GET, HEAD, OPTIONS`.
- Successful download responses are cached at the edge for 120 seconds, keyed by the
  package name and the lower-cased channel (the query string is ignored). They carry
  `Cache-Control: public, max-age=<seconds left>` and `X-Sniff-Cache: HIT` or `MISS`.
  The window is short because the signed download URLs expire and new app versions
  should show up quickly. The Cache API does not work on `*.workers.dev` previews
  or in some local-dev modes; there every request is a `MISS`, and cache errors are
  logged and ignored.
- Error responses carry `Cache-Control: no-store`; the docs page and the OpenAPI
  document are cacheable for 5 minutes.
- Google sessions are reused per Worker instance. If Google rejects one early (HTTP
  401/403), the Worker logs in again and retries that call once.

## Android app

`android-app/` is a companion app for the API. Enter a package ID or a Google Play
link, choose a channel, and the app downloads every file through the API, merges the
splits into a single APK (optionally re-signed), and lets you save or share the APK or
a ZIP of all files.

- Download: [GitHub Releases](https://github.com/madkarmaa/sniff/releases), tags `android-v*`
- Requires Android 8.0 (API 26) or newer
- Build: `./gradlew :app:assembleDebug` inside `android-app/` (needs JDK 17+ and the Android SDK)

## Build from source

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
DEVICE_NAME="px_9a"
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
curl -fsSL -A "Mozilla/5.0" http://localhost:8787/v3/download/com.discord/stable
```

### 3. Checks

```bash
cargo check --all
cargo fmt --all -- --check
cargo clippy --all --locked -- -D warnings
cargo clippy -p sniff --target wasm32-unknown-unknown --locked -- -D warnings
cargo test --workspace --locked
```

CI runs these, plus Android lint and a debug build, on every push and pull request.
Pushes to `v*` branches run the format, Worker clippy, and test checks before
deploying. `Cargo.lock` is committed, so builds use the locked dependency versions.

## Environment variables

`DEVICE_NAME` and the `STABLE_*` pair are required. The `BETA_*` and `ALPHA_*` pairs
are only needed to serve those channels.

- `DEVICE_NAME`: Device identifier for metadata requests (default `px_9a` in `wrangler.toml`); download targets remain independent
- `STABLE_EMAIL`: Email for stable track access
- `STABLE_AAS_TOKEN`: Authentication token for stable track
- `BETA_EMAIL`: Email enrolled in beta programs
- `BETA_AAS_TOKEN`: Authentication token for beta access
- `ALPHA_EMAIL`: Email enrolled in alpha programs
- `ALPHA_AAS_TOKEN`: Authentication token for alpha access
