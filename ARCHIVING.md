# Package version archives

`GET /v1/download/{package}/{channel}/{version_code}` returns Play download info
immediately and queues an archive. The queue downloads `base.apk` and all merged
APK splits, encodes their bytes with the converter, uploads the BMPs to Photos,
and saves a version manifest in D1. Archive failures do not affect the response.
`/v1/details` stays metadata-only.
Play and Photos use the same `{CHANNEL}_EMAIL` / `{CHANNEL}_AAS_TOKEN` pair.
OBB and dex-metadata URLs remain in the response but are not archived.

Each queue delivery processes at most 128 KiB so Workers Free can spread CPU
work across invocations. Parts are
ordinary converter BMPs with the payload length and checksum embedded in pixels.
The final part may be shorter. Small APKs use one BMP. No local APK files are
written by the Worker.

## History and response

D1 stores one record per account, package, channel, and version. The account key
is a hash of the configured email; changing accounts does not reuse another
account's media IDs. D1 contains no credentials. Active jobs temporarily store
signed Play URLs and clear them when complete.
Back up this database: Photos alone does not preserve the APK-to-parts mapping.
Local development records live in `.wrangler/state`; do not delete that directory
if you want to retain local history. Remote deployments use the bound D1 database.

`GET /v1/history/{package}/{channel}` returns the latest 100 records, including
`version_code`, `state`, `created_at`, `updated_at`, `error`, and `photos`.
Older records remain stored in D1. These are archive records, not an access log
of every hit.

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
The download endpoint always returns fresh Play metadata. Use history's `photos`
to recover archived files even if Play no longer serves that version.

The queue advances each version through a D1 lease and checkpoints each part.
Before each Photos mutation, D1 records its BMP SHA-1; after confirmation, it
records the media key. Files already present in the same Photos account are
reused by exact BMP hash. No cloud images are deleted.

`failed` and `uploading` records retain completed parts and any pending hash.
After a crash, queue retries use the D1 checkpoint. A pending Photos commit is
looked up by exact BMP hash; if its result cannot be confirmed, the job stops
for manual inspection. Google Photos and D1 do not share a transaction.

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

For deployment, create a D1 database. The checked-in all-zero ID is for local
development. Set the returned ID in the repository's GitHub Actions variable
`D1_DATABASE_ID`; the deploy workflow inserts it and applies migrations before
deploying:

```sh
bunx wrangler d1 create sniff-history
bunx wrangler queues create sniff-archive
gh variable set D1_DATABASE_ID --repo madkarmaa/sniff --body '<database_id>'
```

For a manual deployment, replace `database_id` in `wrangler.toml` with the
returned ID, then run `bunx wrangler d1 migrations apply sniff-history --remote`
and `bun run deploy`.

Keep the existing channel credentials configured as Worker secrets. The download
endpoint starts a background job; do not prefetch it or cache its responses.
Anyone who can call it can request an archive under the existing access model.
Large archives may use many queue operations; Workers Free includes 10,000 queue
operations per day. Interrupted requests remain visible in history.
The minute cron trigger requeues jobs that have made no progress for at least
one minute, so a stopped queue chain does not require another download request.

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
