package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.concurrent.atomic.AtomicInteger
import java.util.zip.ZipFile
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.apk.XapkWriter
import top.madkarma.sniff.apk.canExportXapk
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.data.SniffRepository

@RunWith(AndroidJUnit4::class)
class XapkExportTest : PipelineTestBase() {
    @Test
    fun xapkHoldsTheOriginalApksAndExpansionFiles() = runBlocking {
        val response = standaloneResponse(installedVersion)
        val repository = SniffRepository(context, fixtureClient({ response }, AtomicInteger()))
        val result = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
        val xapk = File(context.cacheDir, "test.xapk")
        try {
            // A split as the API names it, to check that the archive uses the split ID.
            File(result.folder, "apks/base.apk")
                .copyTo(File(result.folder, "apks/split-0000-config.xxhdpi.apk"))
            assertTrue(result.canExportXapk())

            xapk.outputStream().use { XapkWriter.write(result, it) }

            val packageId = context.packageName
            val obbPath = "Android/obb/$packageId/main.1.$packageId.obb"
            ZipFile(xapk).use { zip ->
                val names = zip.entries().toList().map { it.name }.toSet()
                assertEquals(
                    setOf("manifest.json", "$packageId.apk", "config.xxhdpi.apk", obbPath),
                    names,
                )
                assertArrayEquals(
                    original,
                    zip.getInputStream(zip.getEntry("$packageId.apk")).readBytes(),
                )

                val manifest =
                    JSONObject(
                        zip.getInputStream(zip.getEntry("manifest.json")).reader().readText(),
                    )
                assertEquals(2, manifest.getInt("xapk_version"))
                assertEquals(packageId, manifest.getString("package_name"))
                assertEquals("Test fixture", manifest.getString("name"))
                assertEquals(installedVersion, manifest.getLong("version_code"))
                assertEquals(26, manifest.getInt("min_sdk_version"))
                assertEquals("config.xxhdpi", manifest.getJSONArray("split_configs").getString(0))

                val splitApks = manifest.getJSONArray("split_apks")
                assertEquals(2, splitApks.length())
                assertEquals("$packageId.apk", splitApks.getJSONObject(0).getString("file"))
                assertEquals("base", splitApks.getJSONObject(0).getString("id"))
                assertEquals("config.xxhdpi.apk", splitApks.getJSONObject(1).getString("file"))
                assertEquals("config.xxhdpi", splitApks.getJSONObject(1).getString("id"))

                val expansion = manifest.getJSONArray("expansions").getJSONObject(0)
                assertEquals(obbPath, expansion.getString("file"))
                assertEquals(obbPath, expansion.getString("install_path"))
                assertEquals("EXTERNAL_STORAGE", expansion.getString("install_location"))
            }
        } finally {
            xapk.delete()
            result.folder.deleteRecursively()
        }
    }

    @Test
    fun singleApkWithoutExpansionFilesIsNotBundled() {
        // Like bitpit.launcher: one APK and no splits or expansion files.
        val folder = File(context.cacheDir, "xapk-single-test")
        try {
            File(folder, "apks").mkdirs()
            val apk = File(folder, "single.apk").apply { writeText("fixture") }
            val single =
                DownloadResult(apk, folder, fileCount = 1, merged = false, title = "Single")

            assertFalse(single.canExportXapk())
            assertTrue(single.copy(merged = true).canExportXapk())
        } finally {
            folder.deleteRecursively()
        }
    }
}
