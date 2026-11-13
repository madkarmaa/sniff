package top.madkarma.sniff.data

import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import top.madkarma.sniff.BuildConfig
import top.madkarma.sniff.R
import java.util.concurrent.TimeUnit

const val API_ROOT = "https://sniff.madkarma.top/v3"

private const val API_CALL_TIMEOUT_SECONDS = 120L

internal class SniffApi(private val client: OkHttpClient) {
    suspend fun fetchDownloadPlan(packageId: String, channel: ReleaseChannel): DownloadPlan {
        val endpoint =
            API_ROOT.toHttpUrl().newBuilder().addPathSegment("download").addPathSegment(packageId)
                .addPathSegment(channel.apiName).build()

        val request = Request.Builder().url(endpoint)
            .header("User-Agent", "Sniff-Android/${BuildConfig.VERSION_NAME}").build()

        // The small JSON request gets an overall timeout; newBuilder shares the pool and interceptors.
        val apiClient =
            client.newBuilder().callTimeout(API_CALL_TIMEOUT_SECONDS, TimeUnit.SECONDS).build()

        val data = apiClient.newCall(request).await().use { response ->
            val body = response.body.string()
            val json = runCatching { JSONObject(body) }.getOrNull()

            if (json == null || !response.isSuccessful || !json.optBoolean("success")) {
                // The server's own message, when it sent one, is more precise than the status code.
                val reason = json?.optText("error")
                throw AppException(
                    if (reason != null) UiText.Raw(reason)
                    else uiText(R.string.error_api_failed, response.code)
                )
            }

            json.getJSONObject("data")
        }

        return parseDownloadPlan(packageId, data)
    }
}
