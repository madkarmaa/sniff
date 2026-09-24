// Workers use single-threaded JS futures.
#![allow(clippy::future_not_send)]
//! Bounded-memory APK -> reversible BMP -> Photos pipeline.
use crate::openapi_schema::{ArchivedApk, ArchivedPart, DownloadInfo};
use base64::Engine as _;
use futures::StreamExt as _;
use sha1::Digest as _;
use sha2::Digest as _;
use uploader::client::PhotosClient;
use worker::{Fetch, Request, RequestInit, RequestRedirect, Response, Url};

// Several copies coexist across the WASM/Fetch boundary. Keep well below the
// Workers 128 MiB isolate limit and Photos image size limits, even for large APKs.
const PART_BYTES: usize = 8 * 1024 * 1024;

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        && name != "."
        && name != ".."
}

fn apk_files(info: &DownloadInfo) -> Result<Vec<(String, String)>, String> {
    let main = info
        .main_apk_url
        .as_ref()
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "Missing base APK download URL".to_string())?;
    let mut files = vec![("base.apk".to_string(), main.clone())];
    for split in &info.splits {
        let name = split
            .name
            .as_deref()
            .filter(|name| valid_name(name))
            .ok_or_else(|| "Missing or invalid split name".to_string())?;
        let url = split
            .download_url
            .as_ref()
            .filter(|url| !url.is_empty())
            .ok_or_else(|| format!("Missing download URL for split {name}"))?;
        let filename = format!("{name}.apk");
        if files.iter().any(|(existing, _)| existing == &filename) {
            return Err("Duplicate APK name".to_string());
        }
        files.push((filename, url.clone()));
    }
    // Validate the whole plan before starting any upload.
    for (_, url) in &files {
        delivery_url(url)?;
    }
    Ok(files)
}

fn delivery_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "Invalid APK delivery URL")?;
    let trusted = url.host_str().is_some_and(|host| {
        host == "google.com"
            || host.ends_with(".google.com")
            || host == "googleapis.com"
            || host.ends_with(".googleapis.com")
            || host == "googleusercontent.com"
            || host.ends_with(".googleusercontent.com")
            || host == "gvt1.com"
            || host.ends_with(".gvt1.com")
    });
    if !trusted
        || url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Untrusted APK delivery URL".to_string());
    }
    Ok(url)
}

async fn fetch_apk(raw: &str) -> Result<Response, String> {
    let mut url = delivery_url(raw)?;
    for _ in 0..6 {
        let mut init = RequestInit::new();
        init.with_redirect(RequestRedirect::Manual);
        let request = Request::new_with_init(url.as_str(), &init)
            .map_err(|_| "APK request construction failed")?;
        // Delivery URLs are signed. Never attach Play or Photos credentials.
        let response = Fetch::Request(request)
            .send()
            .await
            .map_err(|_| "APK download failed")?;
        if [301, 302, 303, 307, 308].contains(&response.status_code()) {
            let location = response
                .headers()
                .get("Location")
                .map_err(|_| "Invalid APK redirect")?
                .ok_or("Missing APK redirect")?;
            let next = url.join(&location).map_err(|_| "Invalid APK redirect")?;
            url = delivery_url(next.as_str())?;
            continue;
        }
        if response.status_code() != 200 {
            return Err(format!("APK download HTTP {}", response.status_code()));
        }
        return Ok(response);
    }
    Err("Too many APK redirects".to_string())
}

pub async fn archive(
    package: &str,
    version: i64,
    info: &mut DownloadInfo,
    photos: &mut PhotosClient,
    history: &crate::history::History,
) -> Result<(), String> {
    let files = apk_files(info)?;
    // Authenticate before downloading any potentially large APK.
    photos.bearer_token().await.map_err(|e| e.to_string())?;
    for (name, url) in files {
        let file = ArchivedApk {
            name,
            complete: false,
            pending_bmp_sha1: None,
            bytes: 0,
            sha256: None,
            parts: vec![],
        };
        let name = file.name.clone();
        info.photos.push(file);
        let result = archive_file(package, version, &url, &mut info.photos, photos, history).await;
        if let Err(error) = result {
            return Err(format!(
                "{name}: {error}; completed Photos parts remain available in data.photos"
            ));
        }
    }
    Ok(())
}

async fn archive_file(
    package: &str,
    version: i64,
    url: &str,
    files: &mut [ArchivedApk],
    photos: &mut PhotosClient,
    history: &crate::history::History,
) -> Result<(), String> {
    let mut response = fetch_apk(url).await?;
    let expected = response
        .headers()
        .get("Content-Length")
        .map_err(|_| "Invalid APK length header")?
        .map(|v| v.parse::<u64>().map_err(|_| "Invalid APK length"))
        .transpose()?;
    let mut stream = response.stream().map_err(|_| "APK body unavailable")?;
    let mut buffer = Vec::with_capacity(PART_BYTES);
    let mut hash = sha2::Sha256::new();
    let mut received = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "APK download interrupted")?;
        received = received
            .checked_add(u64::try_from(chunk.len()).map_err(|_| "APK size overflow")?)
            .ok_or("APK size overflow")?;
        if expected.is_some_and(|len| received > len) {
            return Err("APK length mismatch".to_string());
        }
        hash.update(&chunk);
        let mut remaining = chunk.as_slice();
        while !remaining.is_empty() {
            remaining = fill_part(&mut buffer, remaining);
            if buffer.len() == PART_BYTES {
                archive_part(package, version, &buffer, files, photos, history).await?;
                buffer.clear();
            }
        }
    }
    if expected.is_some_and(|len| len != received) || received == 0 {
        return Err("Empty or truncated APK".to_string());
    }
    if !buffer.is_empty() {
        archive_part(package, version, &buffer, files, photos, history).await?;
    }
    let file = files.last_mut().ok_or("Missing archive file")?;
    file.sha256 = Some(crate::history::hex(&hash.finalize()));
    file.complete = true;
    history.save(version, files, "uploading", None).await?;
    Ok(())
}

// Consume only what fits, regardless of the network's chunk boundaries.
fn fill_part<'a>(buffer: &mut Vec<u8>, input: &'a [u8]) -> &'a [u8] {
    let count = PART_BYTES.saturating_sub(buffer.len()).min(input.len());
    let (head, tail) = input.split_at(count);
    buffer.extend_from_slice(head);
    tail
}

async fn archive_part(
    package: &str,
    version: i64,
    payload: &[u8],
    files: &mut [ArchivedApk],
    photos: &mut PhotosClient,
    history: &crate::history::History,
) -> Result<(), String> {
    let file = files.last_mut().ok_or("Missing archive file")?;
    if file.parts.is_empty() && !payload.starts_with(b"PK\x03\x04") {
        return Err("Delivery body is not an APK/ZIP".to_string());
    }
    let bmp = converter::encode(payload).map_err(|_| "BMP encoding failed")?;
    let sha1: [u8; 20] = sha1::Sha1::digest(&bmp).into();
    let bmp_sha1 = crate::history::hex(&sha1);
    let index = file.parts.len();
    let name = format!("{package}-{version}-{}.part{index:05}.bmp", file.name);
    file.pending_bmp_sha1 = Some(bmp_sha1.clone());
    history.save(version, files, "uploading", None).await?;
    let media_key = if let Some(key) = photos
        .find_remote_media_by_hash(&sha1)
        .await
        .map_err(|e| e.to_string())?
    {
        key
    } else {
        let size = u64::try_from(bmp.len()).map_err(|_| "BMP size overflow")?;
        let hash = base64::engine::general_purpose::STANDARD.encode(sha1);
        let token = photos
            .get_upload_token(&hash, size)
            .await
            .map_err(|e| e.to_string())?;
        let scotty = photos
            .put_upload(&token, bmp)
            .await
            .map_err(|e| e.to_string())?;
        let modified = i64::try_from(worker::Date::now().as_millis() / 1000)
            .map_err(|_| "Invalid current time")?;
        photos
            .commit_upload(&scotty, &name, &sha1, modified)
            .await
            .map_err(|e| format!("part {index}, BMP SHA-1 {bmp_sha1}: {e}"))?
    };
    let file = files.last_mut().ok_or("Missing archive file")?;
    file.pending_bmp_sha1 = None;
    file.bytes = file
        .bytes
        .checked_add(u64::try_from(payload.len()).map_err(|_| "APK size overflow")?)
        .ok_or("APK size overflow")?;
    file.parts.push(ArchivedPart {
        index,
        media_key,
        bytes: payload.len(),
        bmp_sha1,
    });
    history.save(version, files, "uploading", None).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_parts_restore_exact_bytes_across_boundaries() {
        let payload = [vec![42; PART_BYTES], vec![17; 8193]].concat();
        // Boundaries smaller than, exactly equal to, and larger than a BMP part.
        for network_chunk in [32771, PART_BYTES, PART_BYTES + 1] {
            let mut buffer = Vec::new();
            let mut restored = Vec::new();
            let mut count = 0;
            for chunk in payload.chunks(network_chunk) {
                let mut remaining = chunk;
                while !remaining.is_empty() {
                    remaining = fill_part(&mut buffer, remaining);
                    assert!(buffer.len() <= PART_BYTES);
                    if buffer.len() == PART_BYTES {
                        let bmp = converter::encode(&buffer).unwrap();
                        restored.extend_from_slice(converter::decode(&bmp).unwrap());
                        buffer.clear();
                        count += 1;
                    }
                }
            }
            let bmp = converter::encode(&buffer).unwrap();
            restored.extend_from_slice(converter::decode(&bmp).unwrap());
            assert_eq!(count, 1);
            assert_eq!(restored, payload);
        }
    }

    #[test]
    fn plan_requires_base_and_every_split_and_trusted_delivery_urls() {
        let mut info = DownloadInfo::from((
            Some("https://play.googleapis.com/a".to_string()),
            vec![(
                Some("config.arm64_v8a".to_string()),
                Some("https://gvt1.com/b".to_string()),
            )],
            vec![],
            None,
        ));
        let files = apk_files(&info).unwrap();
        assert_eq!(
            files
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["base.apk", "config.arm64_v8a.apk"]
        );
        info.splits[0].download_url = None;
        assert!(apk_files(&info).is_err());
        info.splits.clear();
        info.main_apk_url = None;
        assert!(apk_files(&info).is_err());
        for raw in [
            "https://google.com.evil.test/x",
            "http://google.com/x",
            "https://user@google.com/x",
            "https://google.com:444/x",
            "https://localhost/x",
        ] {
            assert!(delivery_url(raw).is_err());
        }
        assert!(!valid_name("../base"));
    }
}
