package top.madkarma.sniff.data

import java.security.MessageDigest
import java.util.Base64
import top.madkarma.sniff.R

/** Outcome of comparing a downloaded file's digest with the hash the API reported. */
internal enum class DigestCheck {
    MATCH,
    MISMATCH,
    SKIPPED,
}

/** The hash to verify a file against: the JCA algorithm name and the string the API reported. */
internal class ExpectedHash(val algorithm: String, val reported: String)

/** SHA-256 when the API gave one, else SHA-1, else null. */
internal fun RemoteFile.expectedHash(): ExpectedHash? =
    sha256?.let { ExpectedHash("SHA-256", it) } ?: sha1?.let { ExpectedHash("SHA-1", it) }

/**
 * Compares [actual] with [reported]. Google Play reports hashes as URL-safe Base64 without padding,
 * but standard Base64 or hex are accepted too, with or without padding and in either case. The
 * reported text is decoded in every encoding; one whose decoded length equals the digest length is
 * a candidate. No candidate means it cannot be this kind of hash, so the check is
 * [DigestCheck.SKIPPED] rather than failed. Hex (64 characters) and Base64 (43 or 44) never collide
 * for a SHA-256.
 */
internal fun checkDigest(actual: ByteArray, reported: String): DigestCheck {
    val text = reported.trim()
    val candidates =
        listOfNotNull(
                decodeHex(text),
                decodeBase64(text, Base64.getDecoder()),
                decodeBase64(text, Base64.getUrlDecoder()),
            )
            .filter { it.size == actual.size }

    return when {
        candidates.isEmpty() -> DigestCheck.SKIPPED
        candidates.any { MessageDigest.isEqual(it, actual) } -> DigestCheck.MATCH
        else -> DigestCheck.MISMATCH
    }
}

/**
 * Throws an [AppException] when a downloaded file of [byteCount] bytes does not match the size or
 * hash the API reported for it. [actualDigest] is the digest of the expected algorithm, if one was
 * computed.
 */
internal fun verifyDownload(file: RemoteFile, byteCount: Long, actualDigest: ByteArray?) {
    if (file.size != null && file.size != byteCount) {
        throw AppException(uiText(R.string.error_file_size, file.label))
    }

    val expected = file.expectedHash()
    if (
        expected != null &&
            actualDigest != null &&
            checkDigest(actualDigest, expected.reported) == DigestCheck.MISMATCH
    ) {
        throw AppException(uiText(R.string.error_file_checksum, file.label))
    }
}

private fun decodeHex(text: String): ByteArray? {
    if (text.length % 2 != 0) return null

    val bytes = ByteArray(text.length / 2)
    for (index in bytes.indices) {
        val high = Character.digit(text[2 * index], 16)
        val low = Character.digit(text[2 * index + 1], 16)
        if (high < 0 || low < 0) return null
        bytes[index] = (high * 16 + low).toByte()
    }
    return bytes
}

/** The decoders accept missing padding; text outside their alphabet makes them throw. */
private fun decodeBase64(text: String, decoder: Base64.Decoder): ByteArray? =
    try {
        decoder.decode(text)
    } catch (_: IllegalArgumentException) {
        null
    }
