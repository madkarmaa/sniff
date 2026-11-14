package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.reandroid.apk.ApkModule
import com.reandroid.archive.block.SignatureId
import java.io.File
import java.util.concurrent.TimeUnit
import java.util.zip.ZipFile
import kotlinx.coroutines.runBlocking
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.data.API_VERSION
import top.madkarma.sniff.data.DEFAULT_SERVER_URL
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.data.SniffRepository

@RunWith(AndroidJUnit4::class)
class CapturedApiPipelineTest : PipelineTestBase() {
    /**
     * Optional replay of captured REAL API JSON; APK/split/dex downloads still go to Google Play.
     */
    @Test
    fun capturedApiResponseDownloadsAndMergesOnDevice() = runBlocking {
        val name = InstrumentationRegistry.getArguments().getString("capturedResponse")
        assumeTrue(
            "Supply -e capturedResponse filename after staging API JSON in target filesDir",
            name != null,
        )
        val captured = File(context.filesDir, name!!).readText()
        val data = JSONObject(captured).getJSONObject("data")
        val packageId =
            data
                .getJSONObject("item")
                .getJSONObject("details")
                .getJSONObject("app_details")
                .getString("package_name")
        val client =
            OkHttpClient.Builder()
                .readTimeout(60, TimeUnit.SECONDS)
                .addInterceptor { chain ->
                    if (
                        chain
                            .request()
                            .url
                            .toString()
                            .startsWith("$DEFAULT_SERVER_URL/$API_VERSION/download/")
                    ) {
                        Response.Builder()
                            .request(chain.request())
                            .protocol(Protocol.HTTP_1_1)
                            .code(200)
                            .message("Captured real API response")
                            .body(captured.toResponseBody("application/json".toMediaType()))
                            .build()
                    } else chain.proceed(chain.request())
                }
                .build()
        val result =
            SniffRepository(context, client).download(
                packageId,
                ReleaseChannel.STABLE,
            ) { message, fraction ->
                println("$message $fraction")
            }
        assertTrue(result.merged)
        assertTrue(result.apk.length() > 0)
        val splits = data.getJSONArray("splits").length()
        assertEquals(splits + 1, File(result.folder, "apks").listFiles()!!.size)
        val dex = data.optString("dex_metadata_url")
        if (dex.isNotBlank() && dex != "null")
            assertTrue(
                File(
                        result.folder,
                        "metadata/base.dm",
                    )
                    .length() > 0,
            )
        ApkModule.loadApkFile(File(result.folder, "apks/base.apk")).use { original ->
            ApkModule.loadApkFile(result.apk).use { merged ->
                val originalSignatures = checkNotNull(original.apkSignatureBlock)
                val mergedSignatures = checkNotNull(merged.apkSignatureBlock)
                originalSignatures
                    .filter { it.id != SignatureId.PADDING }
                    .forEach { signature ->
                        assertArrayEquals(
                            signature.bytes,
                            checkNotNull(mergedSignatures.getSignature(signature.id)).bytes,
                        )
                    }
            }
        }
        ZipFile(result.apk).use { zip ->
            assertNotNull(zip.getEntry("AndroidManifest.xml"))
            assertNotNull(zip.getEntry("classes.dex"))
            assertNotNull(zip.getEntry("resources.arsc"))
        }
        File(context.filesDir, "last-test-result.txt").writeText(result.folder.path)
    }
}
