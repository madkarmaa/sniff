# Google Photos native uploader

Rust library and CLI based on [gotohp](https://github.com/xob0t/gotohp), with
original-download support from observed Android traffic. This uses Photos' private
native protobuf endpoints, not browser cookies or the Photos Library API.

Run from the `sniff` workspace. Set `STABLE_EMAIL` and `STABLE_AAS_TOKEN` in your
environment or a private `.env` file. Only the compiled CLI reads the file; it does
not print credentials. Environment values take precedence. `--env-file PATH`
selects another dotenv file. The AAS must already be exchanged from the one-time
EmbeddedSetup OAuth token.

```sh
cargo run -p uploader -- check-auth
cargo run -p uploader -- upload photo.jpg
cargo run -p uploader -- upload ./photos ./extra.png --recursive --threads 5
cargo run -p uploader -- check ./photos --recursive
cargo run -p uploader -- download --media-key MEDIA_KEY --out original.jpg
```

Upload accepts multiple files/directories. Directory scans are nonrecursive unless
`--recursive` is set; `--exclude NAME` skips matching subdirectories. The default
concurrency is three. Each task owns a client. Hashes already in Photos are skipped;
`--force` explicitly bypasses this check. A failed file makes the batch exit with
failure. Output contains paths, hashes and media keys, without account credentials.

Native download resolves the media key, checks the returned identity, restricts
Bearer authorization to the observed content host, and verifies the original
SHA-1 before writing. Output files are never overwritten. `--url` is also available
for an **exact original URL** from that host, but cannot verify its SHA-1 without
media metadata. It does not append image transformation parameters or accept
arbitrary hosts. Prefer `--media-key` and avoid putting private URLs in shell history.

Existing gotohp query-string credentials remain supported through
`--credential-file PATH` or `GOTOHP_CREDENTIAL`. Token-binding credentials are
rejected; rooted-device token binding is outside this implementation. Raw
`--credential` input remains compatible but a private file/environment avoids
exposing tokens in process arguments.

## File round trip

The existing converter stores a versioned length and SHA-256 inside BMP pixel data.
Use new output paths at every step:

```sh
cargo run -p converter -- encode original.bin encoded.bmp
cargo run -p uploader -- upload encoded.bmp
# Copy the mediaKey from the successful upload line:
cargo run -p uploader -- download --media-key MEDIA_KEY --out downloaded.bmp
cargo run -p converter -- decode downloaded.bmp restored.bin
cmp original.bin restored.bin
```

JPEG and BMP native uploads, exact original downloads, and a binary payload round
trip are verified. PNG, videos, large files and other formats have not been live
verified with this implementation. No cloud items are deleted.

## Library

`uploader::cred::from_aas(email, token)` constructs the upstream credential with an
OS-generated Android ID. `uploader::client::PhotosClient::new(credential)` owns the
HTTP session and refreshes its access token before expiry. No emulator is required.
The library does not read dotenv files.

```rust,no_run
use base64::Engine as _;
use sha1::Digest as _;
use uploader::{client::PhotosClient, cred::from_aas};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let credential = from_aas(
    &std::env::var("STABLE_EMAIL")?,
    &std::env::var("STABLE_AAS_TOKEN")?,
)?;
let mut client = PhotosClient::new(credential)?;
let bytes = std::fs::read("photo.jpg")?;
let sha1: [u8; 20] = sha1::Sha1::digest(&bytes).into();
let id = if let Some(id) = client.find_remote_media_by_hash(&sha1).await? {
    id
} else {
    let digest = base64::engine::general_purpose::STANDARD.encode(sha1);
    let upload_id = client.get_upload_token(&digest, u64::try_from(bytes.len())?).await?;
    let transfer = client.put_upload(&upload_id, bytes).await?;
    client.commit_upload(&transfer, "photo.jpg", &sha1, 1790208000).await?
};
let original = client.download(&id).await?;
# let _ = original;
# Ok(())
# }
```

`Error.completion_uncertain` means a commit may have succeeded. Check Photos/the
file hash before deciding to retry. Matched status 10 without a media result is a
definite rejection; unknown statuses, malformed commit replies and transport
failures after submission remain uncertain. There are no automatic request retries
or redirects. Network errors omit underlying URLs and response bodies.

The client retains gotohp's Pixel XL identity and legacy original-quality commit
shape. No storage-quota benefit is claimed or measured. File bytes are buffered;
large-file memory usage, interrupted-transfer resume and device/account portability
are not established. Requests time out after 120 seconds. Private endpoints can change.

## Provenance and checks

Reference checkout: `/tmp/gotohp-reference`, commit
`97a5dc02db81f5571ce6b586f478600fcd44dd89`. Upstream's MIT notice is retained in
[LICENSE.gotohp](LICENSE.gotohp). No upstream code is executed to handle credentials.
See [../RESEARCH.md](../RESEARCH.md) for live proof and protocol differences.

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
