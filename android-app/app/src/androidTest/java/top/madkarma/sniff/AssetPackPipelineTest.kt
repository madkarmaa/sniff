package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.concurrent.atomic.AtomicInteger
import java.util.zip.ZipFile
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.apk.XapkWriter
import top.madkarma.sniff.data.AssetPack
import top.madkarma.sniff.data.AssetPackDelivery
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.data.SniffRepository

@RunWith(AndroidJUnit4::class)
class AssetPackPipelineTest : PipelineTestBase() {
    /**
     * [standaloneResponse] with one asset pack file per entry of [packs] (name to delivery mode).
     */
    private fun responseWithPacks(vararg packs: Pair<String, String>): String {
        val response = JSONObject(standaloneResponse(installedVersion))
        val files = packs.map { (name, delivery) ->
            JSONObject()
                .put("name", name)
                .put("slice_id", name)
                .put("delivery_mode", delivery)
                .put("download_url", "https://fixture.invalid/$name.apk")
        }
        response.getJSONObject("data").put("asset_packs", JSONArray(files))
        return response.toString()
    }

    @Test
    fun assetPacksAreKeptApartFromTheSplits() = runBlocking {
        val response = responseWithPacks("levels" to "fast_follow", "krisp" to "on_demand")
        val repository = SniffRepository(context, fixtureClient({ response }, AtomicInteger()))
        val result =
            repository.download(
                context.packageName,
                ReleaseChannel.STABLE,
                assetPacks = listOf("krisp"),
            ) { _, _ ->
            }
        val xapk = File(context.cacheDir, "asset-packs.xapk")
        try {
            assertEquals(
                listOf(
                    AssetPack("levels", AssetPackDelivery.FAST_FOLLOW),
                    AssetPack("krisp", AssetPackDelivery.ON_DEMAND),
                ),
                result.assetPacks,
            )
            assertEquals(listOf("base.apk"), File(result.folder, "apks").list()!!.toList())
            assertTrue(File(result.folder, "asset_packs/krisp.apk").isFile)

            xapk.outputStream().use { XapkWriter.write(result, it) }
            ZipFile(xapk).use { zip ->
                assertTrue(zip.getEntry("asset_packs/krisp.apk") != null)
                assertTrue(zip.getEntry("asset_packs/levels.apk") != null)

                val manifest =
                    JSONObject(
                        zip.getInputStream(zip.getEntry("manifest.json")).reader().readText(),
                    )
                // Installers must not try to add the packs as splits.
                assertEquals(1, manifest.getJSONArray("split_apks").length())
                assertEquals(2, manifest.getJSONArray("asset_packs").length())
            }
        } finally {
            xapk.delete()
            result.folder.deleteRecursively()
        }
    }

    @Test
    fun aNewOnDemandPackDownloadsTheAppAgain() = runBlocking {
        var response = responseWithPacks("levels" to "fast_follow")
        val repository = SniffRepository(context, fixtureClient({ response }, AtomicInteger()))
        val first = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
        try {
            val cached = repository.download(context.packageName, ReleaseChannel.STABLE) { _, _ -> }
            assertTrue(cached.cached)
            assertEquals(first.assetPacks, cached.assetPacks)

            response = responseWithPacks("levels" to "fast_follow", "krisp" to "on_demand")
            val withPack =
                repository.download(
                    context.packageName,
                    ReleaseChannel.STABLE,
                    assetPacks = listOf("krisp"),
                ) { _, _ ->
                }
            assertFalse(withPack.cached)
            assertTrue(File(withPack.folder, "asset_packs/krisp.apk").isFile)
        } finally {
            first.folder.deleteRecursively()
        }
    }
}
