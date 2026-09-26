// Workers use single-threaded JS futures.
#![allow(clippy::future_not_send)]
//! Bounded-memory APK -> reversible BMP -> Photos pipeline.
use crate::openapi_schema::{ArchivedApk, ArchivedPart, DownloadInfo};
use base64::Engine as _;
use futures::StreamExt as _;
use sha1::Digest as _;
use sha2::Digest as _;
use sha2::digest::common::hazmat::{SerializableState, SerializedState};
use uploader::client::PhotosClient;
use worker::{Fetch, Request, RequestInit, RequestRedirect, Response, Url};

// Queue consumers have a larger CPU budget than HTTP requests on Workers Free.
const JOB_PART_BYTES: usize = 8 * 1024 * 1024;

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

async fn fetch_apk(raw: &str, range: Option<(u64, usize)>) -> Result<Response, String> {
    let mut url = delivery_url(raw)?;
    for _ in 0..6 {
        let mut init = RequestInit::new();
        init.with_redirect(RequestRedirect::Manual);
        let request = Request::new_with_init(url.as_str(), &init)
            .map_err(|_| "APK request construction failed")?;
        if let Some((offset, size)) = range {
            let end = offset
                .checked_add(u64::try_from(size).map_err(|_| "APK range overflow")?)
                .and_then(|n| n.checked_sub(1))
                .ok_or("APK range overflow")?;
            request
                .headers()
                .set("Range", &format!("bytes={offset}-{end}"))
                .map_err(|_| "APK range request failed")?;
        }
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
        if response.status_code() != if range.is_some() { 206 } else { 200 } {
            return Err(format!("APK download HTTP {}", response.status_code()));
        }
        return Ok(response);
    }
    Err("Too many APK redirects".to_string())
}

async fn fetch_chunk(raw: &str, offset: u64, size: usize) -> Result<(Vec<u8>, bool), String> {
    let mut response = fetch_apk(raw, Some((offset, size))).await?;
    let content_range = response
        .headers()
        .get("Content-Range")
        .map_err(|_| "Invalid APK range header")?
        .ok_or("Missing APK range header")?;
    let (span, total) = content_range
        .strip_prefix("bytes ")
        .and_then(|s| s.split_once('/'))
        .ok_or("Invalid APK range header")?;
    let (start, end) = span.split_once('-').ok_or("Invalid APK range header")?;
    let start: u64 = start.parse().map_err(|_| "Invalid APK range start")?;
    let end: u64 = end.parse().map_err(|_| "Invalid APK range end")?;
    let total: u64 = total.parse().map_err(|_| "Invalid APK range total")?;
    let length = end
        .checked_sub(start)
        .and_then(|n| n.checked_add(1))
        .ok_or("Invalid APK range")?;
    let after_end = end.checked_add(1).ok_or("Invalid APK range")?;
    if start != offset
        || end >= total
        || length > u64::try_from(size).map_err(|_| "APK range overflow")?
    {
        return Err("Unexpected APK range".to_string());
    }
    let mut body = Vec::with_capacity(size);
    let mut stream = response.stream().map_err(|_| "APK body unavailable")?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "APK download interrupted")?;
        if body.len().saturating_add(chunk.len()) > size {
            return Err("APK range too large".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    if u64::try_from(body.len()).map_err(|_| "APK range overflow")? != length {
        return Err("APK range truncated".to_string());
    }
    Ok((body, after_end == total))
}

fn restore_hash(state: Option<&str>) -> Result<sha2::Sha256, String> {
    let Some(state) = state else {
        return Ok(sha2::Sha256::new());
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(state)
        .map_err(|_| "Invalid archive hash state")?;
    let serialized = SerializedState::<sha2::Sha256>::try_from(bytes.as_slice())
        .map_err(|_| "Invalid archive hash state")?;
    sha2::Sha256::deserialize(&serialized).map_err(|_| "Invalid archive hash state".to_string())
}

pub async fn archive_step(
    package: &str,
    version: i64,
    plan: &DownloadInfo,
    files: &mut Vec<ArchivedApk>,
    job: &crate::history::Job,
    photos: &mut PhotosClient,
    history: &crate::history::History,
) -> Result<bool, String> {
    let planned = apk_files(plan)?;
    let index = files.iter().take_while(|file| file.complete).count();
    if index == planned.len() && files.len() == index {
        return Ok(true);
    }
    let (name, url) = planned.get(index).ok_or("Archive file mismatch")?;
    if files.len() == index {
        files.push(ArchivedApk {
            name: name.clone(),
            complete: false,
            pending_bmp_sha1: None,
            bytes: 0,
            sha256: None,
            parts: vec![],
        });
    }
    let file = files.get(index).ok_or("Archive file missing")?;
    if files.len() != index.saturating_add(1) || &file.name != name {
        return Err("Archive checkpoint requires manual recovery".to_string());
    }
    if job.hash_bytes > file.bytes {
        return Err("Invalid archive hash offset".to_string());
    }
    let size = if job.hash_bytes < file.bytes {
        usize::try_from(
            file.bytes
                .saturating_sub(job.hash_bytes)
                .min(u64::try_from(JOB_PART_BYTES).map_err(|_| "APK range overflow")?),
        )
        .map_err(|_| "APK range overflow")?
    } else {
        JOB_PART_BYTES
    };
    let (chunk, last) = fetch_chunk(url, job.hash_bytes, size).await?;
    if job.hash_bytes == 0 && !chunk.starts_with(b"PK\x03\x04") {
        return Err("Delivery body is not an APK/ZIP".to_string());
    }
    let mut hash = restore_hash(job.hash_state.as_deref())?;
    hash.update(&chunk);
    let next_offset = job
        .hash_bytes
        .checked_add(u64::try_from(chunk.len()).map_err(|_| "APK size overflow")?)
        .ok_or("APK size overflow")?;
    if job.hash_bytes == file.bytes {
        archive_part(package, version, &chunk, files, photos, history).await?;
    } else if next_offset > file.bytes {
        return Err("Invalid archive hash offset".to_string());
    }
    if last {
        if files.get(index).ok_or("Archive file missing")?.bytes != next_offset {
            return Err("APK length mismatch".to_string());
        }
        if files
            .get(index)
            .ok_or("Archive file missing")?
            .pending_bmp_sha1
            .is_some()
        {
            return Err("Pending Photos commit requires manual recovery".to_string());
        }
        let file = files.get_mut(index).ok_or("Archive file missing")?;
        file.sha256 = Some(crate::history::hex(&hash.finalize()));
        file.complete = true;
        let initial = sha2::Sha256::new().serialize();
        history
            .save_progress(
                version,
                files,
                &base64::engine::general_purpose::STANDARD.encode(initial),
                0,
            )
            .await?;
    } else {
        history
            .save_progress(
                version,
                files,
                &base64::engine::general_purpose::STANDARD.encode(hash.serialize()),
                next_offset,
            )
            .await?;
    }
    Ok(files.len() == planned.len() && files.iter().all(|file| file.complete))
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
    let pending = file.pending_bmp_sha1.is_some();
    if file
        .pending_bmp_sha1
        .as_deref()
        .is_some_and(|previous| previous != bmp_sha1)
    {
        return Err("Pending Photos hash differs from the current APK part".to_string());
    }
    if !pending {
        file.pending_bmp_sha1 = Some(bmp_sha1.clone());
        history.save(version, files, "uploading", None).await?;
    }
    let found = photos
        .find_remote_media_by_hash(&sha1)
        .await
        .map_err(|e| e.to_string())?;
    let media_key = if let Some(key) = found {
        key
    } else if pending {
        return Err(
            "Pending Photos commit is not visible by hash; inspect before retrying".to_string(),
        );
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
