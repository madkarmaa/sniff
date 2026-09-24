//! Minimal protobuf encode/decode for the Google Photos endpoints used here.
//!
//! Based on `gotohp` (`core/api.go`, `.proto/*.proto`, `generated/*.pb.go`):
//! the app speaks protobuf-over-HTTPS while spoofing a Pixel XL, it does not
//! use gRPC services. Only the messages needed for single-file upload are
//! implemented.

use std::collections::HashMap;

/// Pixel XL identity copied from `gotohp/core/api.go::newAPIFromCredential`.
pub const PIXEL_MODEL: &str = "Pixel XL";
/// Device maker copied from `gotohp/core/api.go`.
pub const PIXEL_MAKE: &str = "Google";
/// Android API level copied from `gotohp/core/api.go` (`androidAPIVersion: 28`).
pub const ANDROID_API_VERSION: i64 = 28;
/// Photos client version copied from `gotohp/core/api.go`.
pub const CLIENT_VERSION_CODE: i64 = 49_029_607;
/// Build fingerprint fragment from `gotohp`'s user agent.
pub const BUILD_FINGERPRINT: &str = "PQ2A.190205.001";
/// Legacy commit `field4.field2` constant from `gotohp/core/api.go::CommitUpload`.
pub const COMMIT_UNKNOWN_INT: i64 = 46_000_000;
/// Original-quality storage policy (`qualityVal = 3`) from `gotohp`.
pub const QUALITY_ORIGINAL: i64 = 3;
/// `CommitUpload.field3` constant bytes from `gotohp/core/api.go`.
pub const COMMIT_FIELD3: &[u8] = &[1, 3];

const WIRE_VARINT: u64 = 0;
const WIRE_BYTES: u64 = 2;

/// A decoded protobuf field.
#[derive(Debug, Clone)]
pub struct ProtoField {
    /// Field number.
    pub number: u32,
    /// Wire type (0 = varint, 2 = length-delimited).
    pub wire: u8,
    /// Varint value when `wire == 0`.
    pub varint: Option<u64>,
    /// Raw bytes when `wire == 2`.
    pub bytes: Option<Vec<u8>>,
}

/// Encode a `u64` as protobuf varint.
///
/// # Errors
///
/// Returns an error if an intermediate conversion fails (unreachable for
/// `u64` inputs, kept fallible for lint-clean arithmetic).
pub fn encode_varint_to(value: u64, out: &mut Vec<u8>) -> Result<(), String> {
    let mut rest = value;
    for _ in 0..10 {
        let low_bits = rest & 0x7F_u64;
        let shifted = rest
            .checked_shr(7)
            .ok_or_else(|| "varint shift failed".to_string())?;
        rest = shifted;
        if rest == 0 {
            let byte = u8::try_from(low_bits).map_err(|e| format!("varint byte: {e}"))?;
            out.push(byte);
            return Ok(());
        }
        let with_cont = low_bits | 0x80_u64;
        let byte = u8::try_from(with_cont).map_err(|e| format!("varint byte: {e}"))?;
        out.push(byte);
    }
    Err("varint too long".to_string())
}

/// Decode a varint at the start of `data`.
///
/// # Errors
///
/// Returns an error on truncation, overflow, or overlong encodings.
pub fn decode_varint(data: &[u8]) -> Result<(u64, usize), String> {
    let mut result: u64 = 0;
    for index in 0..10 {
        let byte = *data
            .get(index)
            .ok_or_else(|| "truncated varint".to_string())?;
        if index == 9 && byte > 1 {
            return Err("varint overflow".into());
        }
        let low = u64::from(byte & 0x7F_u8);
        let shift = u32::try_from(index)
            .map_err(|e| format!("varint index: {e}"))?
            .checked_mul(7)
            .ok_or_else(|| "varint shift overflow".to_string())?;
        let placed = low
            .checked_shl(shift)
            .ok_or_else(|| "varint overflow".to_string())?;
        // Reject non-canonical encodings that would set bits already decided.
        if (result & placed) != 0 {
            return Err("overlapping varint bits".to_string());
        }
        result = result.wrapping_add(placed);
        if byte & 0x80_u8 == 0 {
            let consumed = index
                .checked_add(1)
                .ok_or_else(|| "varint len".to_string())?;
            // Canonical form check: minimal encoding only.
            let mut check = Vec::new();
            encode_varint_to(result, &mut check)?;
            let got = data
                .get(0..consumed)
                .ok_or_else(|| "varint slice".to_string())?;
            if got != check.as_slice() {
                return Err("non-canonical varint".to_string());
            }
            return Ok((result, consumed));
        }
    }
    Err("varint too long".to_string())
}

fn encode_tag_to(field: u32, wire: u64, out: &mut Vec<u8>) -> Result<(), String> {
    let tag = u64::from(field)
        .checked_mul(8)
        .ok_or_else(|| "tag overflow".to_string())?
        .checked_add(wire)
        .ok_or_else(|| "tag overflow".to_string())?;
    encode_varint_to(tag, out)
}

/// Append a varint field.
///
/// # Errors
///
/// Returns an error if the tag or value cannot be encoded.
pub fn varint_field_to(field: u32, value: u64, out: &mut Vec<u8>) -> Result<(), String> {
    encode_tag_to(field, WIRE_VARINT, out)?;
    encode_varint_to(value, out)
}

/// Append a length-delimited field.
///
/// # Errors
///
/// Returns an error if the tag or length cannot be encoded.
pub fn bytes_field_to(field: u32, data: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    encode_tag_to(field, WIRE_BYTES, out)?;
    let len = u64::try_from(data.len()).map_err(|e| format!("field len: {e}"))?;
    encode_varint_to(len, out)?;
    out.extend_from_slice(data);
    Ok(())
}

/// Parse all top-level fields of a protobuf message.
///
/// # Errors
///
/// Returns an error on truncation or malformed tags/values.
pub fn parse_fields(data: &[u8]) -> Result<Vec<ProtoField>, String> {
    let mut fields = Vec::new();
    let mut pos: usize = 0;
    while pos < data.len() {
        let rest = data.get(pos..).ok_or_else(|| "field slice".to_string())?;
        let (tag, tag_len) = decode_varint(rest)?;
        let number = tag.checked_shr(3).ok_or_else(|| "tag shift".to_string())?;
        let number = u32::try_from(number).map_err(|e| format!("field number: {e}"))?;
        if number == 0 || number >= (1 << 29) {
            return Err("invalid protobuf field".into());
        }
        let wire_raw = tag & 0x07_u64;
        let wire = u8::try_from(wire_raw).map_err(|e| format!("wire type: {e}"))?;
        pos = pos
            .checked_add(tag_len)
            .ok_or_else(|| "tag position".to_string())?;
        if wire == 0 {
            let rest = data.get(pos..).ok_or_else(|| "varint slice".to_string())?;
            let (value, used) = decode_varint(rest)?;
            pos = pos
                .checked_add(used)
                .ok_or_else(|| "value pos".to_string())?;
            fields.push(ProtoField {
                number,
                wire,
                varint: Some(value),
                bytes: None,
            });
        } else if wire == 2 {
            let rest = data.get(pos..).ok_or_else(|| "len slice".to_string())?;
            let (len, used) = decode_varint(rest)?;
            pos = pos.checked_add(used).ok_or_else(|| "len pos".to_string())?;
            let len = usize::try_from(len).map_err(|e| format!("field len: {e}"))?;
            let end = pos
                .checked_add(len)
                .ok_or_else(|| "field end".to_string())?;
            let bytes = data
                .get(pos..end)
                .ok_or_else(|| "truncated field".to_string())?;
            pos = end;
            fields.push(ProtoField {
                number,
                wire,
                varint: None,
                bytes: Some(bytes.to_vec()),
            });
        } else if wire == 1 {
            let end = pos
                .checked_add(8)
                .ok_or_else(|| "fixed64 end".to_string())?;
            data.get(pos..end)
                .ok_or_else(|| "truncated fixed64".to_string())?;
            pos = end;
        } else if wire == 5 {
            let end = pos
                .checked_add(4)
                .ok_or_else(|| "fixed32 end".to_string())?;
            data.get(pos..end)
                .ok_or_else(|| "truncated fixed32".to_string())?;
            pos = end;
        } else {
            return Err(format!("unsupported wire type {wire} for field {number}"));
        }
    }
    Ok(fields)
}

fn first_bytes(fields: &[ProtoField], number: u32) -> Option<&[u8]> {
    for field in fields {
        if field.number == number
            && let Some(bytes) = field.bytes.as_deref()
        {
            return Some(bytes);
        }
    }
    None
}

fn first_varint(fields: &[ProtoField], number: u32) -> Option<u64> {
    for field in fields {
        if field.number == number
            && let Some(value) = field.varint
        {
            return Some(value);
        }
    }
    None
}

/// Build `GetUploadToken{2,2,1,3,size}` (`gotohp/core/api.go::GetUploadToken`).
///
/// # Errors
///
/// Returns an error if the file size does not fit or encoding fails.
pub fn encode_get_upload_token(file_size: u64) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    varint_field_to(1, 2, &mut out)?;
    varint_field_to(2, 2, &mut out)?;
    varint_field_to(3, 1, &mut out)?;
    varint_field_to(4, 3, &mut out)?;
    varint_field_to(7, file_size, &mut out)?;
    Ok(out)
}

/// Build `HashCheck{1:{1:{1:sha1},2:{}}}` (`gotohp/core/api.go`).
///
/// # Errors
///
/// Returns an error if encoding fails.
pub fn encode_hash_check(sha1: &[u8; 20]) -> Result<Vec<u8>, String> {
    let mut inner = Vec::new();
    bytes_field_to(1, sha1, &mut inner)?;
    let mut middle = Vec::new();
    bytes_field_to(1, &inner, &mut middle)?;
    bytes_field_to(2, &[], &mut middle)?;
    let mut outer = Vec::new();
    bytes_field_to(1, &middle, &mut outer)?;
    Ok(outer)
}

/// Build the legacy `CommitUpload` body (`gotohp/core/api.go::CommitUpload`).
///
/// `commit_field1`/`commit_field2` come from decoding the Scotty finalize
/// token; the device is always the Pixel XL identity above.
///
/// # Errors
///
/// Returns an error if any integer conversion or encoding fails.
#[allow(clippy::too_many_arguments)]
pub fn encode_commit_upload(
    commit_field1: i64,
    commit_field2: &[u8],
    file_name: &str,
    sha1: &[u8; 20],
    modified_unix: i64,
    model: &str,
    make: &str,
    android_api_version: i64,
    quality: i64,
) -> Result<Vec<u8>, String> {
    let commit_field1_u = u64::try_from(commit_field1).map_err(|e| format!("commit f1: {e}"))?;
    let modified_u = u64::try_from(modified_unix).map_err(|e| format!("mtime: {e}"))?;
    let unknown_u = u64::try_from(COMMIT_UNKNOWN_INT).map_err(|e| format!("unknown int: {e}"))?;
    let api_u = u64::try_from(android_api_version).map_err(|e| format!("api version: {e}"))?;
    let quality_u = u64::try_from(quality).map_err(|e| format!("quality: {e}"))?;

    let mut token_inner = Vec::new();
    varint_field_to(1, commit_field1_u, &mut token_inner)?;
    bytes_field_to(2, commit_field2, &mut token_inner)?;

    let mut stamp = Vec::new();
    varint_field_to(1, modified_u, &mut stamp)?;
    varint_field_to(2, unknown_u, &mut stamp)?;

    let mut field1 = Vec::new();
    bytes_field_to(1, &token_inner, &mut field1)?;
    bytes_field_to(2, file_name.as_bytes(), &mut field1)?;
    bytes_field_to(3, sha1, &mut field1)?;
    bytes_field_to(4, &stamp, &mut field1)?;
    varint_field_to(7, quality_u, &mut field1)?;
    varint_field_to(10, 1, &mut field1)?;

    let mut device = Vec::new();
    bytes_field_to(3, model.as_bytes(), &mut device)?;
    bytes_field_to(4, make.as_bytes(), &mut device)?;
    varint_field_to(5, api_u, &mut device)?;

    let mut outer = Vec::new();
    bytes_field_to(1, &field1, &mut outer)?;
    bytes_field_to(2, &device, &mut outer)?;
    bytes_field_to(3, COMMIT_FIELD3, &mut outer)?;
    Ok(outer)
}

/// Validate a Scotty finalize token and return its opaque `field2` bytes.
///
/// The envelope must be exactly `{1:2, 2:opaque}` (`gotohp/core/scotty_token.go`).
///
/// # Errors
///
/// Returns an error if the shape, version, or counts are wrong.
pub fn scotty_opaque(raw: &[u8]) -> Result<Vec<u8>, String> {
    let fields = parse_fields(raw)?;
    let mut version_count: usize = 0;
    let mut opaque_count: usize = 0;
    let mut opaque: Option<Vec<u8>> = None;
    for field in &fields {
        if field.number == 1 && field.wire == 0 {
            if field.varint == Some(2) {
                version_count = version_count
                    .checked_add(1)
                    .ok_or_else(|| "version count".to_string())?;
            } else {
                return Err("unsupported Scotty token version".to_string());
            }
        } else if field.number == 2
            && field.wire == 2
            && let Some(bytes) = field.bytes.clone()
        {
            if bytes.is_empty() {
                return Err("empty Scotty field2".to_string());
            }
            opaque = Some(bytes);
            opaque_count = opaque_count
                .checked_add(1)
                .ok_or_else(|| "opaque count".to_string())?;
        }
    }
    if version_count != 1 {
        return Err(format!("Scotty field1 count {version_count}, want 1"));
    }
    if opaque_count != 1 {
        return Err(format!("Scotty field2 count {opaque_count}, want 1"));
    }
    opaque.ok_or_else(|| "missing Scotty opaque bytes".to_string())
}

/// Decode a legacy `CommitToken{field1, field2}`.
///
/// # Errors
///
/// Returns an error on malformed input or failed conversions.
pub fn decode_commit_token(raw: &[u8]) -> Result<(i64, Vec<u8>), String> {
    let fields = parse_fields(raw)?;
    let f1 = first_varint(&fields, 1).ok_or_else(|| "missing commit field1".to_string())?;
    let f2 = first_bytes(&fields, 2).ok_or_else(|| "missing commit field2".to_string())?;
    let f1 = i64::try_from(f1).map_err(|e| format!("commit f1: {e}"))?;
    Ok((f1, f2.to_vec()))
}

/// Extract the media key from a `CreateMediaItemsResponse` body.
///
/// Requires one item, a matching transfer token and explicit success.
/// A matched status 10 without a result is a definite rejection.
///
/// # Errors
///
/// Returns an error when no media key is present.
pub fn commit_media_key(response: &[u8], transferred: &[u8]) -> Result<Option<String>, String> {
    let outer = parse_fields(response)?;
    let item = parse_fields(unique_bytes(&outer, 1)?)?;
    if unique_bytes(&item, 1)? != transferred {
        return Err("commit token mismatch".into());
    }
    let codes: Vec<_> = item.iter().filter(|f| f.number == 2).collect();
    let [status] = codes.as_slice() else {
        return Err("missing or ambiguous commit status".into());
    };
    if status.varint == Some(10) && !item.iter().any(|f| f.number == 3) {
        return Ok(None);
    }
    if status.varint != Some(0) {
        return Err("unknown commit status".into());
    }
    let result = parse_fields(unique_bytes(&item, 3)?)?;
    let key = std::str::from_utf8(unique_bytes(&result, 1)?)
        .map_err(|_| "invalid media key".to_string())?;
    if key.is_empty() {
        return Err("missing media key".into());
    }
    Ok(Some(key.into()))
}

/// Validate a hash lookup, distinguishing an absent item from an invalid response.
/// # Errors
/// Rejects malformed responses, ambiguous fields and a mismatched fingerprint.
pub fn remote_media_key(response: &[u8], sha1: &[u8; 20]) -> Result<Option<String>, String> {
    let outer = parse_fields(response)?;
    let envelope = parse_fields(unique_bytes(&outer, 1)?)?;
    let matched = parse_fields(unique_bytes(&envelope, 2)?)?;
    let fingerprint = parse_fields(unique_bytes(&matched, 1)?)?;
    if unique_bytes(&fingerprint, 1)? != sha1 {
        return Err("lookup fingerprint mismatch".into());
    }
    if !matched.iter().any(|f| f.number == 2) {
        return Ok(None);
    }
    let media = parse_fields(unique_bytes(&matched, 2)?)?;
    let id = std::str::from_utf8(unique_bytes(&media, 1)?)
        .map_err(|_| "invalid media key".to_string())?;
    if id.is_empty() {
        return Err("missing media key".into());
    }
    Ok(Some(id.into()))
}

fn unique_bytes(fields: &[ProtoField], number: u32) -> Result<&[u8], String> {
    let mut found = fields.iter().filter(|f| f.number == number);
    let field = found.next().ok_or("missing protobuf field")?;
    if found.next().is_some() {
        return Err("ambiguous protobuf field".into());
    }
    field
        .bytes
        .as_deref()
        .ok_or_else(|| "wrong protobuf field type".into())
}

fn nested(data: &[u8], path: &[u32]) -> Result<Vec<u8>, String> {
    let mut current = data.to_vec();
    for number in path {
        current = unique_bytes(&parse_fields(&current)?, *number)?.to_vec();
    }
    Ok(current)
}

/// Observed `PhotosPrepareDownload` request for one media ID.
/// # Errors
/// Rejects an empty/oversized ID or an encoding failure.
pub fn prepare_download(media_id: &str) -> Result<Vec<u8>, String> {
    if media_id.is_empty() || media_id.len() > 1024 {
        return Err("invalid media ID".into());
    }
    let mut key = Vec::new();
    bytes_field_to(1, media_id.as_bytes(), &mut key)?;
    let mut entry = Vec::new();
    bytes_field_to(1, &key, &mut entry)?;
    let mut request = Vec::new();
    bytes_field_to(1, &entry, &mut request)?;
    let mask = [
        10, 4, 58, 2, 18, 0, 42, 10, 18, 0, 26, 0, 42, 4, 10, 0, 24, 0,
    ];
    bytes_field_to(2, &mask, &mut request)?;
    Ok(request)
}

/// Get the original URL and fingerprint, verifying the requested media ID.
/// # Errors
/// Rejects incomplete, ambiguous or mismatched metadata.
pub fn original_download(response: &[u8], id: &str) -> Result<(String, [u8; 20]), String> {
    if nested(response, &[1, 1])? != id.as_bytes() {
        return Err("download media mismatch".into());
    }
    let sha1 = nested(response, &[1, 2, 13, 1])?
        .try_into()
        .map_err(|_| "invalid original SHA1".to_string())?;
    let url = String::from_utf8(nested(response, &[1, 5, 2, 5])?)
        .map_err(|_| "invalid original URL".to_string())?;
    Ok((url, sha1))
}

/// Parse a `key=value` per-line auth response into a map.
///
/// Mirrors `gotohp/core/api.go::getAuthToken` response parsing.
#[must_use]
pub fn parse_auth_response(body: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(field: u32, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        bytes_field_to(field, bytes, &mut out).unwrap();
        out
    }
    #[test]
    fn commit_checks_token_status_and_ambiguity() {
        let token = [8, 2, 18, 1, 7];
        let mut item = wrap(1, &token);
        item.extend([16, 0]);
        item.extend(wrap(3, &wrap(1, b"media")));
        assert_eq!(
            commit_media_key(&wrap(1, &item), &token).unwrap(),
            Some("media".into())
        );
        assert!(commit_media_key(&wrap(1, &item), b"wrong").is_err());
        let mut duplicate = wrap(1, &item);
        duplicate.extend(wrap(1, &item));
        assert!(commit_media_key(&duplicate, &token).is_err());
        let mut rejected = wrap(1, &token);
        rejected.extend([16, 10]);
        assert!(
            commit_media_key(&wrap(1, &rejected), &token)
                .unwrap()
                .is_none()
        );
        let mut missing = wrap(1, &token);
        missing.extend(wrap(3, &wrap(1, b"media")));
        assert!(commit_media_key(&wrap(1, &missing), &token).is_err());
    }
    #[test]
    fn malformed_hash_lookup_is_not_a_missing_image() {
        assert!(remote_media_key(&[], &[0; 20]).is_err());
        let fingerprint = wrap(1, &[7; 20]);
        let matched = wrap(1, &fingerprint);
        let response = wrap(1, &wrap(2, &matched));
        assert!(remote_media_key(&response, &[7; 20]).unwrap().is_none());
        assert!(remote_media_key(&response, &[8; 20]).is_err());
        assert!(parse_fields(&[0]).is_err());
        assert!(decode_varint(&[255, 255, 255, 255, 255, 255, 255, 255, 255, 2]).is_err());
        assert!(parse_fields(&[10, 10, 1]).is_err());
    }
    #[test]
    fn download_metadata_matches_media_and_hash() {
        assert_eq!(prepare_download(&"a".repeat(44)).unwrap().len(), 70);
        let mut media = wrap(1, b"media");
        media.extend(wrap(2, &wrap(13, &wrap(1, &[7; 20]))));
        media.extend(wrap(
            5,
            &wrap(2, &wrap(5, b"https://lh3.googleusercontent.com/original")),
        ));
        let reply = wrap(1, &media);
        assert_eq!(original_download(&reply, "media").unwrap().1, [7; 20]);
        assert!(original_download(&reply, "other").is_err());
    }

    #[test]
    fn varint_roundtrip() {
        for value in [0_u64, 1, 127, 128, 300, 46_000_000, u64::MAX] {
            let mut buf = Vec::new();
            encode_varint_to(value, &mut buf).unwrap();
            let (back, used) = decode_varint(&buf).unwrap();
            assert_eq!(back, value);
            assert_eq!(used, buf.len());
        }
    }

    #[test]
    fn upload_token_shape() {
        let body = encode_get_upload_token(12).unwrap();
        let fields = parse_fields(&body).unwrap();
        assert_eq!(first_varint(&fields, 1), Some(2));
        assert_eq!(first_varint(&fields, 7), Some(12));
    }

    #[test]
    fn scotty_rejects_bad_version() {
        let mut bad = Vec::new();
        varint_field_to(1, 9, &mut bad).unwrap();
        bytes_field_to(2, &[1, 2, 3], &mut bad).unwrap();
        assert!(scotty_opaque(&bad).is_err());
    }
}
