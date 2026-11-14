package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.data.SniffRepository
import top.madkarma.sniff.data.normalizeServerUrl

@RunWith(AndroidJUnit4::class)
class ServerSettingsTest : PipelineTestBase() {
    @Test
    fun acceptsHttpsServerUrls() {
        assertEquals("https://sniff.example.com", normalizeServerUrl(" https://sniff.example.com/ "))
        assertEquals("https://example.com/sniff", normalizeServerUrl("https://example.com/sniff/"))
        assertEquals("https://example.com:8443", normalizeServerUrl("HTTPS://Example.com:8443"))
    }

    @Test
    fun rejectsUnusableServerUrls() {
        listOf(
            "",
            "sniff.example.com",
            "http://sniff.example.com",
            "ftp://sniff.example.com",
            "https://sniff.example.com/?key=value",
            "https://sniff.example.com/#docs",
        ).forEach { assertNull(it, normalizeServerUrl(it)) }
    }

    @Test
    fun lookupsGoToTheConfiguredServer() = runBlocking {
        var requested: String? = null
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            requested = chain.request().url.toString()
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(503)
                .message("Fixture").body("{}".toResponseBody()).build()
        }.build()
        val repository =
            SniffRepository(context, client, serverUrl = { "https://example.com/sniff" })
        try {
            repository.download(context.packageName, ReleaseChannel.BETA) { _, _ -> }
            fail("A failed lookup must not succeed")
        } catch (_: Exception) {
            // Only the request URL matters here.
        }
        assertEquals(
            "https://example.com/sniff/v3/download/${context.packageName}/beta", requested
        )
    }
}
