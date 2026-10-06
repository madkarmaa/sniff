package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.MediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer
import okio.BufferedSource
import okio.Source
import okio.Timeout
import okio.buffer
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.data.SniffRepository
import java.io.File

@RunWith(AndroidJUnit4::class)
class DownloadFailureTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private fun metadata(): JSONObject = JSONObject().put("success", true).put(
        "data", JSONObject().put(
            "item", JSONObject().put(
                "details", JSONObject().put(
                    "app_details",
                    JSONObject().put("package_name", context.packageName).put("version_code", 1)
                )
            )
        ).put("main_apk_url", "https://fixture.invalid/base.apk").put("splits", JSONArray())
            .put("additional_files", JSONArray())
    )

    private fun folders(): Set<String> =
        File(context.cacheDir, "downloads").listFiles()?.map { it.name }?.toSet().orEmpty()

    @Test
    fun missingRequiredSplitUrlFailsBeforeDownloading() = runBlocking {
        val before = folders()
        val metadata = metadata().apply {
            getJSONObject("data").put(
                "splits", JSONArray().put(
                    JSONObject().put("name", "config.en").put("download_url", JSONObject.NULL)
                )
            )
        }.toString()
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            assertEquals("sniff.madkarma.top", chain.request().url.host)
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(200)
                .message("Fixture").body(metadata.toResponseBody()).build()
        }.build()
        try {
            SniffRepository(context, client).download(context.packageName, ReleaseChannel.STABLE) {}
            fail("A required split URL must not be silently skipped")
        } catch (expected: IllegalStateException) {
            assertTrue(expected.message.orEmpty().contains("required download URL"))
        }
        assertEquals(before, folders())
    }

    @Test
    fun cancellingInProgressDownloadRemovesItsTemporaryFolder() = runBlocking {
        val before = folders()
        val bytes = File(context.applicationInfo.sourceDir).readBytes()
        val metadata = metadata().toString()
        val firstByte = CompletableDeferred<Unit>()
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            val body =
                if (chain.request().url.host == "sniff.madkarma.top") metadata.toResponseBody() else object :
                    ResponseBody() {
                    private val data = Buffer().write(bytes)
                    private val throttled = object : Source {
                        override fun read(sink: Buffer, byteCount: Long): Long {
                            Thread.sleep(25)
                            return data.read(sink, minOf(byteCount, 4096))
                        }

                        override fun timeout(): Timeout = Timeout.NONE
                        override fun close() {
                            data.close()
                        }
                    }.buffer()

                    override fun contentType(): MediaType? = null
                    override fun contentLength(): Long = bytes.size.toLong()
                    override fun source(): BufferedSource = throttled
                }
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(200)
                .message("Fixture").body(body).build()
        }.build()
        val job = launch {
            SniffRepository(context, client).download(
                context.packageName, ReleaseChannel.STABLE
            ) { progress ->
                if (progress.startsWith("Downloading")) firstByte.complete(Unit)
            }
            fail("Cancelled download must not return a result")
        }
        withTimeout(5_000) { firstByte.await() }
        job.cancelAndJoin()
        assertEquals(before, folders())
    }
}
