package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.data.SniffRepository

@RunWith(AndroidJUnit4::class)
class DownloadCacheTest : PipelineTestBase() {
    @Test
    fun sameVersionIsServedFromCacheWithoutDownloading() = runBlocking {
        val response = standaloneResponse(installedVersion)
        val downloads = AtomicInteger()
        val repository = SniffRepository(context, fixtureClient({ response }, downloads))
        val first = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
        try {
            assertEquals(3, downloads.get())
            val second = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
            assertTrue(second.cached)
            assertEquals(3, downloads.get())
            assertEquals(first.apk, second.apk)
            assertEquals(first.fileCount, second.fileCount)
        } finally {
            first.folder.deleteRecursively()
        }
    }

    @Test
    fun cacheWithUnversionedApkNameIsRenamedWithoutDownloading() = runBlocking {
        val response = standaloneResponse(installedVersion)
        val downloads = AtomicInteger()
        val repository = SniffRepository(context, fixtureClient({ response }, downloads))
        val first = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
        try {
            // Recreate the layout written by app versions that named the APK by package only.
            val legacy = File(first.folder, "${context.packageName}.apk")
            assertTrue(first.apk.renameTo(legacy))
            val inventory = File(first.folder, "inventory.json")
            inventory.writeText(
                JSONObject(inventory.readText()).put("output", legacy.name).toString(),
            )

            val second = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
            assertTrue(second.cached)
            assertEquals(3, downloads.get())
            assertEquals(first.apk, second.apk)
            assertFalse(legacy.exists())
            assertArrayEquals(original, second.apk.readBytes())
            assertEquals(second.apk.name, JSONObject(inventory.readText()).getString("output"))
        } finally {
            first.folder.deleteRecursively()
        }
    }

    @Test
    fun failedUpdateKeepsTheCachedVersion() = runBlocking {
        var response = standaloneResponse(installedVersion)
        val downloads = AtomicInteger()
        val repository = SniffRepository(context, fixtureClient({ response }, downloads))
        val cached = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
        try {
            // A newer version code makes the downloaded APK fail its version check.
            response = standaloneResponse(installedVersion + 1)
            try {
                repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
                fail("An APK that does not match the API version must be rejected")
            } catch (expected: IllegalStateException) {
                assertTrue(expected.message.orEmpty().contains("does not match"))
            }
            assertEquals(6, downloads.get())
            val leftovers =
                cached.folder.parentFile!!.list()!!.filter {
                    it.startsWith("${context.packageName}-")
                }
            assertEquals(emptyList<String>(), leftovers)
            assertTrue(cached.apk.isFile)

            response = standaloneResponse(installedVersion)
            val reused = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
            assertTrue(reused.cached)
            assertEquals(6, downloads.get())
            assertArrayEquals(original, reused.apk.readBytes())
        } finally {
            cached.folder.deleteRecursively()
        }
    }
}
