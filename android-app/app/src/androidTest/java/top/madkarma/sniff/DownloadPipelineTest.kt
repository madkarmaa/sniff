package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.android.apksig.ApkVerifier
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.data.AppException
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.data.SniffRepository
import top.madkarma.sniff.data.UiText
import java.io.File
import java.security.MessageDigest
import java.util.Base64
import java.util.concurrent.atomic.AtomicInteger

@RunWith(AndroidJUnit4::class)
class DownloadPipelineTest : PipelineTestBase() {
    /** Genuine signed standalone APK (this app), plus expansion and dex files; no split fallback. */
    @Test
    fun standaloneApkPreservesSignatureAndDownloadsAdditionalFiles() = runBlocking {
        val response = standaloneResponse(installedVersion)
        val client = fixtureClient({ response }, AtomicInteger())
        val result = SniffRepository(context, client).download(
            context.packageName, ReleaseChannel.STABLE, resign = true
        ) { _, _ -> }
        try {
            assertFalse(result.merged)
            assertFalse(result.cached)
            assertEquals(3, result.fileCount)
            assertEquals("${context.packageName}_$installedVersion.apk", result.apk.name)
            assertArrayEquals(
                MessageDigest.getInstance("SHA-256").digest(original),
                MessageDigest.getInstance("SHA-256").digest(result.apk.readBytes())
            )
            assertTrue(File(result.folder, "additional/main.1.${context.packageName}.obb").exists())
            assertTrue(File(result.folder, "metadata/base.dm").exists())
            assertTrue(ApkVerifier.Builder(result.apk).build().verify().isVerified)
        } finally {
            result.folder.deleteRecursively()
        }
    }

    /** API v3 adds sizes, hashes and per-ABI targets; failed targets are reported, not fatal. */
    @Test
    fun v3SizeAndHashAreCheckedAndFailedTargetsReported() = runBlocking {
        val apk = original
        val digest = Base64.getUrlEncoder().withoutPadding()
            .encodeToString(MessageDigest.getInstance("SHA-256").digest(apk))
        val targets = JSONArray().put(
            JSONObject().put("abi", "arm64-v8a").put("success", true).put("error", JSONObject.NULL)
        ).put(JSONObject().put("abi", "armeabi-v7a").put("success", false).put("error", "unavailable"))
        val response = JSONObject(standaloneResponse(installedVersion)).apply {
            getJSONObject("data").put("main_apk_size", apk.size).put("main_apk_sha256", digest)
                .put("targets", targets)
        }.toString()
        val result = SniffRepository(context, fixtureClient({ response }, AtomicInteger())).download(
            context.packageName, ReleaseChannel.STABLE
        ) { _, _ -> }
        try {
            assertEquals(listOf("armeabi-v7a"), result.failedAbis)
        } finally {
            result.folder.deleteRecursively()
        }
    }

    @Test
    fun wrongReportedHashRejectsTheDownloadAndKeepsNoFiles() = runBlocking {
        val response = JSONObject(standaloneResponse(installedVersion)).apply {
            getJSONObject("data").put("main_apk_sha256", "00".repeat(32))
        }.toString()
        try {
            SniffRepository(context, fixtureClient({ response }, AtomicInteger())).download(
                context.packageName, ReleaseChannel.STABLE
            ) { _, _ -> }
            fail("A file that does not match its reported hash must be rejected")
        } catch (expected: AppException) {
            assertEquals(R.string.error_file_checksum, (expected.text as UiText.Resource).id)
        }
        val leftovers = File(context.cacheDir, "downloads").list().orEmpty()
            .filter { it.startsWith("${context.packageName}-") }
        assertEquals(emptyList<String>(), leftovers)
    }
}
