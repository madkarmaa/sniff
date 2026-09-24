//! Reversible binary payloads in valid 24-bit BMP images.
use sha2::{Digest, Sha256};
use std::io;

const MAGIC: &[u8; 8] = b"BMPFILE1";
const HEADER_LEN: usize = 54;
const PREFIX_LEN: usize = 102;
const MIN_SIDE: usize = 32;
const MAGIC_OFFSET: usize = HEADER_LEN;
const LEN_OFFSET: usize = 62;
const HASH_OFFSET: usize = 70;

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid, truncated, or corrupted converter BMP",
    )
}

fn read_bytes(buf: &[u8], offset: usize, len: usize) -> io::Result<&[u8]> {
    let end = offset.checked_add(len).ok_or_else(invalid)?;
    buf.get(offset..end).ok_or_else(invalid)
}

fn write_bytes(buf: &mut [u8], offset: usize, src: &[u8]) -> io::Result<()> {
    let end = offset.checked_add(src.len()).ok_or_else(invalid)?;
    let slot = buf.get_mut(offset..end).ok_or_else(invalid)?;
    slot.copy_from_slice(src);
    Ok(())
}

fn read_u32(buf: &[u8], offset: usize) -> io::Result<u32> {
    Ok(u32::from_le_bytes(
        read_bytes(buf, offset, 4)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}

fn write_u32(buf: &mut [u8], offset: usize, value: u32) -> io::Result<()> {
    write_bytes(buf, offset, &value.to_le_bytes())
}

/// Encode bytes as a valid BMP containing their length and SHA-256.
/// # Errors
/// Returns an error if the payload exceeds the BMP format limits.
pub fn encode(data: &[u8]) -> io::Result<Vec<u8>> {
    let used = data
        .len()
        .checked_add(PREFIX_LEN - HEADER_LEN)
        .ok_or_else(invalid)?;

    let root = used
        .div_ceil(3)
        .isqrt()
        .checked_add(1)
        .ok_or_else(invalid)?;

    let width = root.next_multiple_of(4).max(MIN_SIDE);
    let stride = width.checked_mul(3).ok_or_else(invalid)?;
    let height = used.div_ceil(stride).max(MIN_SIDE);
    let image_size = stride.checked_mul(height).ok_or_else(invalid)?;
    let total = image_size.checked_add(HEADER_LEN).ok_or_else(invalid)?;
    let size_u32 = u32::try_from(total).map_err(|_| invalid())?;
    let width_u32 = u32::try_from(width).map_err(|_| invalid())?;
    let height_u32 = u32::try_from(height).map_err(|_| invalid())?;
    let image_u32 = u32::try_from(image_size).map_err(|_| invalid())?;
    let data_len_u64 = u64::try_from(data.len()).map_err(|_| invalid())?;

    // A nearly square 24-bit bitmap; width multiple of four eliminates row padding.
    let mut bmp = vec![0; total];

    write_bytes(&mut bmp, 0, b"BM")?;
    write_u32(&mut bmp, 2, size_u32)?;
    write_u32(
        &mut bmp,
        10,
        u32::try_from(HEADER_LEN).map_err(|_| invalid())?,
    )?;
    write_u32(&mut bmp, 14, 40)?;
    write_u32(&mut bmp, 18, width_u32)?;
    write_u32(&mut bmp, 22, height_u32)?;
    write_bytes(&mut bmp, 26, &1u16.to_le_bytes())?;
    write_bytes(&mut bmp, 28, &24u16.to_le_bytes())?;
    write_u32(&mut bmp, 34, image_u32)?;
    write_bytes(&mut bmp, MAGIC_OFFSET, MAGIC)?;
    write_bytes(&mut bmp, LEN_OFFSET, &data_len_u64.to_le_bytes())?;
    write_bytes(&mut bmp, HASH_OFFSET, &Sha256::digest(data))?;
    write_bytes(&mut bmp, PREFIX_LEN, data)?;

    Ok(bmp)
}

/// Validate a converter BMP and borrow its original payload.
/// # Errors
/// Rejects malformed images, changed payloads, and nonzero trailing padding.
pub fn decode(bmp: &[u8]) -> io::Result<&[u8]> {
    let total = usize::try_from(read_u32(bmp, 2)?).map_err(|_| invalid())?;
    let header = usize::try_from(read_u32(bmp, 10)?).map_err(|_| invalid())?;

    if bmp.len() < PREFIX_LEN
        || read_bytes(bmp, 0, 2)? != b"BM"
        || total != bmp.len()
        || header != HEADER_LEN
        || read_u32(bmp, 14)? != 40
        || read_bytes(bmp, 26, 4)? != [1, 0, 24, 0]
        || read_u32(bmp, 30)? != 0
        || read_bytes(bmp, HEADER_LEN, MAGIC.len())? != MAGIC
    {
        return Err(invalid());
    }

    let width = read_u32(bmp, 18)?;
    let height = read_u32(bmp, 22)?;
    let max_dim = u32::try_from(i32::MAX).map_err(|_| invalid())?;

    if width == 0 || height == 0 || width % 4 != 0 || width > max_dim || height > max_dim {
        return Err(invalid());
    }

    let expected = u64::from(width)
        .checked_mul(3)
        .and_then(|row| row.checked_mul(u64::from(height)))
        .ok_or_else(invalid)?;

    let body_len = bmp.len().checked_sub(HEADER_LEN).ok_or_else(invalid)?;

    if expected != u64::try_from(body_len).map_err(|_| invalid())?
        || u64::from(read_u32(bmp, 34)?) != expected
    {
        return Err(invalid());
    }

    let len = usize::try_from(u64::from_le_bytes(
        read_bytes(bmp, LEN_OFFSET, 8)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
    .map_err(|_| invalid())?;

    let end = PREFIX_LEN.checked_add(len).ok_or_else(invalid)?;
    let data = bmp.get(PREFIX_LEN..end).ok_or_else(invalid)?;

    if Sha256::digest(data)[..] != *read_bytes(bmp, HASH_OFFSET, 32)?
        || bmp.get(end..).ok_or_else(invalid)?.iter().any(|&v| v != 0)
    {
        return Err(invalid());
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_roundtrip_and_rejection() {
        for len in [0, 1, 2, 3, 939, 3024, 3025, 10_000] {
            let data: Vec<u8> = (0..len)
                .map(|i| u8::try_from(i % 256).expect("i % 256 fits in u8"))
                .collect();

            let bmp = encode(&data).unwrap();
            assert_eq!(decode(&bmp).unwrap(), data);
            assert!(decode(&bmp[..bmp.len() - 1]).is_err());

            let mut corrupt = bmp.clone();
            corrupt[70] ^= 1;
            assert!(decode(&corrupt).is_err());

            let mut bad_length = bmp.clone();
            bad_length[62..70].fill(255);
            assert!(decode(&bad_length).is_err());
        }

        assert!(decode(b"not an image").is_err());
    }
}
