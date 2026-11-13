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

private val UNSAFE_FILENAME_CHARACTERS = Regex("[^A-Za-z0-9._-]")
private const val MAX_FILENAME_LENGTH = 160

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

    private fun parseDownloadPlan(packageId: String, data: JSONObject): DownloadPlan {
        val item = data.getJSONObject("item")

        val details = item.getJSONObject("details").getJSONObject("app_details")
        check(details.getString("package_name") == packageId) { "API returned a different package" }

        val versionCode = details.getLong("version_code")

        val files = mutableListOf(
            RemoteFile(
                label = "base.apk",
                url = requiredUrl(data, "main_apk_url"),
                relativePath = "apks/base.apk",
                size = data.optSize("main_apk_size"),
                sha1 = data.optText("main_apk_sha1"),
                sha256 = data.optText("main_apk_sha256"),
            )
        )

        val splitCount = addSplits(data, files)
        addAdditionalFiles(data, files)

        data.optText("dex_metadata_url")
            ?.let { files += RemoteFile("base.dm", it, "metadata/base.dm") }

        return DownloadPlan(
            packageId = packageId,
            versionCode = versionCode,
            title = item.optText("title") ?: packageId,
            files = files.toList(),
            splitCount = splitCount,
            failedAbis = failedAbis(data),
        )
    }

    /** The ABIs the server could not fetch, from the optional per-ABI "targets" of API v3. */
    private fun failedAbis(data: JSONObject): List<String> {
        val targets = data.optJSONArray("targets") ?: return emptyList()

        return (0 until targets.length()).mapNotNull { index ->
            val target = targets.optJSONObject(index) ?: return@mapNotNull null
            // A target that does not say it failed is not reported as failed.
            if (target.optBoolean("success", true)) null else target.optText("abi")
        }.distinct()
    }

    private fun addSplits(data: JSONObject, files: MutableList<RemoteFile>): Int {
        val splits = data.getJSONArray("splits")

        for (index in 0 until splits.length()) {
            val split = splits.getJSONObject(index)
            val name = safeFileName(split.optText("name") ?: "split-$index")
            val numberedName = index.toString().padStart(4, '0')

            files += RemoteFile(
                label = "$name.apk",
                url = requiredUrl(split, "download_url"),
                relativePath = "apks/split-$numberedName-$name.apk",
                size = split.optSize("size"),
                sha1 = split.optText("sha1"),
                sha256 = split.optText("sha256"),
            )
        }

        return splits.length()
    }

    private fun addAdditionalFiles(data: JSONObject, files: MutableList<RemoteFile>) {
        val additional = data.getJSONArray("additional_files")
        val names = mutableSetOf<String>()

        for (index in 0 until additional.length()) {
            val file = additional.getJSONObject(index)

            val name = safeFileName(file.optText("filename") ?: "additional-$index")
            check(names.add(name)) { "API returned duplicate additional filenames" }

            files += RemoteFile(
                label = name,
                url = requiredUrl(file, "download_url"),
                relativePath = "additional/$name",
                size = file.optSize("size"),
                sha1 = file.optText("sha1"),
                sha256 = file.optText("sha256"),
            )
        }
    }

    private fun requiredUrl(json: JSONObject, key: String): String =
        checkNotNull(json.optText(key)) { "API omitted a required download URL ($key)" }

    private fun safeFileName(value: String): String =
        value.replace(UNSAFE_FILENAME_CHARACTERS, "_").take(MAX_FILENAME_LENGTH)
            .takeIf { it.isNotBlank() && it != "." && it != ".." } ?: "file"
}

/** Returns the value of [key], or null when it is missing, JSON null, or blank. */
private fun JSONObject.optText(key: String): String? =
    if (isNull(key)) null else optString(key).takeIf { it.isNotBlank() }

/** Returns the byte count at [key], or null when it is missing, JSON null, or not a positive number. */
private fun JSONObject.optSize(key: String): Long? =
    if (isNull(key)) null else optLong(key, -1L).takeIf { it > 0 }
