package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import java.security.MessageDigest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.data.AppException
import top.madkarma.sniff.data.DigestCheck
import top.madkarma.sniff.data.RemoteFile
import top.madkarma.sniff.data.UiText
import top.madkarma.sniff.data.checkDigest
import top.madkarma.sniff.data.verifyDownload

@RunWith(AndroidJUnit4::class)
class IntegrityTest {
    // SHA-256 of the ASCII bytes "abc", in the encodings Google Play may use.
    private val hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    private val base64 = "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
    private val base64Url = "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0"
    private val digest = MessageDigest.getInstance("SHA-256").digest("abc".toByteArray())

    @Test
    fun acceptsHexAndBase64InAnyCaseAndPadding() {
        listOf(
                hex,
                hex.uppercase(),
                " $hex\n",
                base64,
                base64.trimEnd('='),
                base64Url,
                base64Url + "=",
            )
            .forEach { assertEquals(it, DigestCheck.MATCH, checkDigest(digest, it)) }
    }

    @Test
    fun skipsTextThatCannotBeThisDigest() {
        listOf(
                "",
                "not a hash!",
                "zz",
                // A SHA-1 or a truncated hash is not a SHA-256.
                "a9993e364706816aba3e25717850c26c9cd0d89d",
                hex.dropLast(2),
                base64Url.dropLast(4),
            )
            .forEach { assertEquals(it, DigestCheck.SKIPPED, checkDigest(digest, it)) }
    }

    @Test
    fun rejectsAWrongDigestInEveryEncoding() {
        val wrong = MessageDigest.getInstance("SHA-256").digest("abd".toByteArray())
        listOf(
                hex.replaceRange(0, 2, "bb"),
                base64.replaceRange(0, 1, "v"),
                base64Url.replaceRange(0, 1, "v"),
            )
            .forEach { assertEquals(it, DigestCheck.MISMATCH, checkDigest(digest, it)) }
        assertEquals(DigestCheck.MISMATCH, checkDigest(wrong, hex))
    }

    @Test
    fun verifyDownloadChecksSizeThenHash() {
        val file = RemoteFile("a.apk", "https://example.invalid/a", "a.apk", size = 3, sha256 = hex)

        verifyDownload(file, 3, digest)
        // Without a computed digest, or without a size, only what is available is checked.
        verifyDownload(file, 3, null)
        verifyDownload(file.copy(size = null), 7, digest)
        // The reported text is not a SHA-256, so the check is skipped.
        verifyDownload(file.copy(sha256 = "garbage"), 3, digest)

        val size = assertThrows(AppException::class.java) { verifyDownload(file, 4, digest) }
        assertEquals(R.string.error_file_size, (size.text as UiText.Resource).id)

        val wrong = MessageDigest.getInstance("SHA-256").digest("abd".toByteArray())
        val hash = assertThrows(AppException::class.java) { verifyDownload(file, 3, wrong) }
        assertEquals(R.string.error_file_checksum, (hash.text as UiText.Resource).id)
    }

    @Test
    fun sha1IsOnlyUsedWithoutSha256() {
        val sha1 = MessageDigest.getInstance("SHA-1").digest("abc".toByteArray())
        val both = RemoteFile("a.apk", "u", "a.apk", sha1 = "0".repeat(40), sha256 = hex)
        // A wrong SHA-1 is ignored because the SHA-256 is given; the digest passed is that of
        // SHA-256.
        verifyDownload(both, 3, digest)

        val only =
            RemoteFile("a.apk", "u", "a.apk", sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d")
        verifyDownload(only, 3, sha1)
        val bad = only.copy(sha1 = "0".repeat(40))
        assertThrows(AppException::class.java) { verifyDownload(bad, 3, sha1) }
    }
}
