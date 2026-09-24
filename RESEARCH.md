# Native Photos implementation proof — 2026-09-24

The final project is `sniff/uploader`, continuing the user's gotohp-based Rust port.
The earlier `gphotos-research/gphotos` experiment is not the final implementation.

## Reference and authentication

Cloned https://github.com/xob0t/gotohp into `/tmp/gotohp-reference` at
`97a5dc02db81f5571ce6b586f478600fcd44dd89`. Inspected `core/googleauth.go`,
`core/api.go`, `core/upload.go`, `core/scotty_token.go` and the relevant protobuf
schemas. Retained the upstream MIT notice in `uploader/LICENSE.gotohp`.

`buildGooglePhotosCredential` supplies a random 8-byte hex Android ID, `Token`
(the existing AAS), `Email`, the Photos package as app/callerPkg, Photos signature
`24bb24c05e47e0aefa68a58a766179d9b613a600` as client_sig/callerSig, SDK 33,
oauth2_foreground=1 and the openid/mobileapps.native/photos.native scopes.
`getAuthToken` exchanges this form at `https://android.googleapis.com/auth`.
This complete flow succeeded with the supplied AAS, without an emulator/check-in.

The previous experiment's ServiceDisabled result used a different signature,
EncryptedPasswd form and auth host. Its conclusion that an independently registered
Android ID was required does not apply to this verified upstream credential flow.
No single-field cause is claimed: the complete upstream form was tested.

The compiled CLI alone read the authorized existing dotenv file at runtime.
The agent did not open, source, display or inspect it. Auth forms, tokens, raw
responses and signed URLs remain in memory. Errors carry only sanitized stage,
HTTP/native status and finalization certainty. The library never reads dotenv files.

## Native flow

- POST `photosdata-pa.googleapis.com/6439526531001121323/5084965799730810217`:
  SHA-1 lookup. The parser requires a matching echoed hash; malformed/empty replies
  are errors, not permission to upload.
- POST `photos.googleapis.com/data/upload/uploadmedia/interactive`: gotohp's
  legacy start body `{1:2,2:2,3:1,4:3,7:size}`, SHA-1 header and upload length.
  Extract `X-GUploader-UploadID` without logging it.
- PUT the same endpoint with that upload ID and original bytes; validate the
  returned Scotty token. Query encoding prevents upload-ID URL injection.
- POST `photosdata-pa.googleapis.com/6439526531001121323/16538846908252377752`:
  gotohp's legacy commit body and Pixel XL/Google/API 28 metadata. Require the echoed
  transfer token, explicit status 0 and a media key. The upstream legacy shape
  omits CRC32C; JPEG and BMP commits were both accepted. This is distinct from the
  newer captured Android shape, where retaining an incorrect CRC32C caused rejection.
- POST `photosdata-pa.googleapis.com/$rpc/social.frontend.photos.preparedownloaddata.v1.PhotosPrepareDownloadDataService/PhotosPrepareDownload`:
  observed request with media ID at `[1,1,1]` and field-selection message at `[2]`.
  Verify response ID `[1,1]`; read original SHA-1 `[1,2,13,1]` and URL `[1,5,2,5]`.
- GET that exact original URL with Bearer authorization. Require HTTPS, the exact
  `lh3.googleusercontent.com` host, no userinfo/nonstandard port/fragment, HTTP 200
  and image content type. Do not follow redirects or add preview transformations.
  Verify SHA-1 before exclusively creating the local output.

## Live results

Using only the compiled Rust CLI and supplied AAS at runtime:

| Fixture | Bytes | Media key | Original/download SHA-256 |
| --- | ---: | --- | --- |
| `rust-native-upload.jpg` | 13928 | `AF1QipMboFa5c0eLGFdSSsTUd7HnV7lu5OY0FtJ5hjYe` | `0fb5168a38ca6cf632516645b587d4905223eadbdd0a75a4e3e55b943ad21757` |
| `sniff-native-roundtrip.bmp` | 3126 | `AF1QipO7eisUm-Rf5ZM6PemC4Anzphs6yRp0hJ9xsnDy` | `d02b17c6169815bccf6a724f8798cae1bdfbb1a81e558d3042ab89e12c44cca3` |

Both uploads returned validated successful commits. Independent native hash checks
with two concurrent tasks found both media keys. Native prepare-download and original
GET produced exact byte-for-byte files, independently confirmed by `sha256sum`.
This proves persisted library items, not just accepted upload bytes.

The BMP was created by the workspace converter from a fresh payload with text,
NUL and non-ASCII bytes. After native upload/download, the converter restored it.
Original and restored payload SHA-256:
`87ea1d0eaa1b0840d6a6652dbf8db9fa27b42b9505a5d3969e76ea0d2b2bb71f`.
Fixtures and restored outputs are in the adjacent `gphotos-research` directory.
No original was overwritten, and no cloud item was deleted.

The prior research emulator was started for an additional UI check, but its Photos
package did not resolve a launch activity. No new Photos UI confirmation is claimed;
persistence and integrity were established through independent native lookup and
download operations. No credential or instrumentation was installed in that emulator.

## Boundaries

Keep the batch scan/concurrency/force interface. Same-size file modifications are
caught by rehashing the bytes before transfer. Recursive scans track visited directories
to avoid symlink cycles. HTTP redirects and automatic retries are disabled.
Known matched rejection status 10 is definite; unknown commit replies and errors
after submission are uncertain. Do not blindly retry them.

This is a private-protocol client, not an OAuth Photos Library API client. Live proof
covers JPEG, BMP, native original downloads and exact payload recovery. Videos, PNG,
large files, storage-quota behavior, long-lived token refresh and cross-account
portability remain unverified. Token-binding credentials are not supported.

## Final checks

`cargo fmt --all --check`, `cargo test --workspace` (19 tests), and
`cargo clippy --workspace --all-targets -- -D warnings` passed. Tests cover native
wire shapes, malformed input, commit-token/status validation, hash-lookup mismatch,
download identity, Bearer host restrictions, AAS form construction, same-size file
changes, CLI selectors and the converter round trip. No external service is contacted
by offline tests. The verification-only emulator was stopped afterward.

## API package archives and durable version history (2026-09-24)

The requested final flow is implemented in the Worker: `/v1/download` retrieves
Play's merged base/split APKs, converts their bytes to reversible BMP parts, uploads
to Photos with the same channel email/AAS credentials, and checkpoints a D1 version
manifest. `/v1/details` remains read-only. `/v1/history/{package}/{channel}` exposes
stored versions. Account, package, channel, and version form the history key;
credentials and expiring Play/Photos URLs are not stored in D1. See ARCHIVING.md.

A real request for `com.google.android.calculator`, stable version `85022643`,
returned HTTP 200 after archiving all three APKs returned by the merged Play flow.
Play did not offer x86/x86_64 builds for this requested version; these were skipped
by the existing delivery merge. The API used `.dev.vars` through Wrangler at
runtime; the compiled uploader then loaded the same file at runtime to download
every Photos original. No credential values were inspected or displayed by the agent.

| APK | Bytes | Source / restored SHA-256 | Photos media key |
| --- | ---: | --- | --- |
| base.apk | 7298875 | `86131a534d5da0ce35b30e13ed5a6ab2f12e6c67f71a721d05937d5c2708bbaa` | `AF1QipNKIt6Q_H2ZXIGMTHeUv1g6sJyxYZk2JlZMEe0o` |
| config.xxhdpi.apk | 47496 | `1c8bdd22d925b02eec76c7f86307f0c7513755ea25c3592da0ec896a5015c8d1` | `AF1QipMSnj-_Z283pe89lSCHdwyAIddLfAxVdLKrFXDT` |
| config.xhdpi.apk | 43401 | `74cb8db3c3f2ffc1a3ce07713c0c10c3ba3b49ded1ac975eca3dc02def5f5b93` | `AF1QipMHM7oXgWcxvZEKbYci1Plg3vXaF7luEWRvTaxu` |

Every BMP was downloaded by the native Rust uploader, SHA-1 checked against the
manifest, and decoded by the converter. All recovered APK sizes and SHA-256 hashes
matched the bytes streamed from Play. The sanitized manifest is also in local D1.
Proof artifacts were left in `/tmp/sniff-apk-proof-tmrq8vp0`.

After stopping and restarting the Worker process, requesting the same version
returned the identical three media keys and unchanged history timestamps in
58 ms (including the history request), without fresh Play URLs. Isolated runtime
tests additionally block all egress and verify that completed and incomplete
history records cannot initiate another Play/Photos flow. D1 records survive a
full runtime restart; concurrent claims produce exactly one owner; another
account's records do not appear in history.

The initial Worker auth probe exposed a runtime-specific transport issue:
Cloudflare rejects Fetch `redirect: "error"` during Request construction. A
credential-free workerd probe reproduced that TypeError; `manual` constructed
successfully. The uploader now uses `manual` on WASM and rejects non-200 responses
at the protocol layer, preserving the no-redirect Bearer boundary. No uploads
occurred in those initial failed attempts; their empty local test records were
removed before the successful verification. Wrangler now watches the converter
and uploader source directories as well as the API so library fixes are rebuilt.

Limitations: this is locally verified, not deployed. The D1 binding has a local-only
ID until a deployment database is created and migrated. APKs above 8 MiB use ordered
parts; boundary reconstruction is tested with multiple network chunk sizes, while
the live fixture's APKs each fit one part. The existing Worker access model is
unchanged, so access controls and production CPU/subrequest limits must suit the
operator's deployment. Failed/interrupted attempts remain available for inspection;
automatic recovery of uncertain Photos mutations is intentionally not implemented.

Final checks passed: `cargo fmt --all --check`, `cargo test --workspace` (21 tests),
`cargo clippy --workspace --all-targets -- -D warnings`, and `bun run test:worker`
(release WASM build plus isolated D1/Worker integration tests).
