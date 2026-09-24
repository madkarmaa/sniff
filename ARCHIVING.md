# Package version archives

`GET /v1/download/{package}/{channel}/{version_code}` downloads `base.apk` and
all merged APK splits, encodes their bytes with the converter, uploads the BMPs
to Photos, and saves a version manifest in D1. `/v1/details` stays metadata-only.
Play and Photos use the same `{CHANNEL}_EMAIL` / `{CHANNEL}_AAS_TOKEN` pair.
OBB and dex-metadata URLs remain in the response but are not archived.

Each file is processed sequentially in parts of at most 8 MiB. This supports
large APKs within Workers' memory limit and Photos' image limits. Parts are
ordinary converter BMPs with the payload length and checksum embedded in pixels.
The final part may be shorter. Small APKs use one BMP. No local APK files are
written by the Worker.

## History and response

D1 stores one record per account, package, channel, and version. The account key
is a hash of the configured email; changing accounts does not reuse another
account's media IDs. D1 contains no credentials or signed download URLs.
Back up this database: Photos alone does not preserve the APK-to-parts mapping.
Local development records live in `.wrangler/state`; do not delete that directory
if you want to retain local history. Remote deployments use the bound D1 database.

`GET /v1/history/{package}/{channel}` returns the latest 100 records, including
`version_code`, `state`, `created_at`, `updated_at`, `error`, and `photos`.
Older records remain stored and can be retrieved by requesting their version
through `/v1/download`. These are archive records, not an access log of every hit.

Example `data.photos` entry (illustrative IDs/hashes):

```json
{
  "name": "base.apk",
  "complete": true,
  "pending_bmp_sha1": null,
  "bytes": 1234,
  "sha256": "APK_SHA256",
  "parts": [
    { "index": 0, "media_key": "PHOTOS_MEDIA_KEY", "bytes": 1234, "bmp_sha1": "BMP_SHA1" }
  ]
}
```

Success requires every file to finish and the final manifest to be persisted.
A subsequent request for a completed version returns that manifest directly;
`main_apk_url` and `dex_metadata_url` are null, and `splits` / `additional_files`
are empty. Use `photos` to recover the archived files, even if Play no longer
serves that version. The history preserves all merged split names.

A unique database key elects one request to upload a version across isolates.
Before each Photos mutation, D1 records its BMP SHA-1; after confirmation, it
records the media key. Files already present in the same Photos account are
reused by exact BMP hash. No cloud images are deleted.

Failures return HTTP 502 with `success: false`, an error, and the partial manifest
in `data.photos`. `failed` and `uploading` records are retained, including completed
parts and any pending hash. A crash can leave `uploading` indefinitely. Subsequent
requests return the record rather than blindly retrying a possibly successful
commit. Inspect the pending hash with the uploader before manually recovering
an interrupted record; automatic takeover/retry is not implemented. Google Photos
and D1 do not share a transaction, so a checkpoint failure can leave an uploaded
image whose media key is not yet recorded. The pre-submission hash allows lookup.
If the attempt has no parts or pending hash and failed before uploading, it is
safe for an operator to remove that failed record and request the version again.

## Restore an APK

Use the same account as the archive. For every part, ordered by `index`:

```sh
cargo run -p uploader -- download --media-key PHOTOS_MEDIA_KEY --out part00000.bmp
cargo run -p converter -- decode part00000.bmp part00000.bin
```

Concatenate the decoded parts into a **new** APK file in index order. Compare its
size and SHA-256 with the file manifest. The uploader checks the original BMP's
SHA-1 and the converter checks the embedded payload SHA-256. Never decode Photos
previews or resized images. Do not overwrite existing APKs.

## Setup

For local development:

```sh
bun install
bunx wrangler d1 migrations apply sniff-history --local
bun run dev
```

For deployment, create a D1 database and place its returned `database_id` in
`wrangler.toml` (the checked-in all-zero ID is local-only):

```sh
bunx wrangler d1 create sniff-history
# Set database_id in wrangler.toml, then:
bunx wrangler d1 migrations apply sniff-history --remote
bun run deploy
```

Keep the existing channel credentials configured as Worker secrets. Archive
creation now makes `/v1/download` a mutating, potentially long-running request;
do not prefetch it or cache its responses. The API still uses the repository's
existing access model; anyone who can call this endpoint can request an archive.
Large packages require adequate Workers CPU/subrequest limits. Interrupted
requests remain visible in history and need inspection before recovery.

## Verification

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bun run test:worker
```

The Worker tests use isolated D1 storage and dummy credentials, block all external
network access, and verify history across restarts, account isolation, atomic
ownership, completed-version reuse, and preservation of uncertain outcomes.
Live package upload/download proof is recorded in [RESEARCH.md](RESEARCH.md).
