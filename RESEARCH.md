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
