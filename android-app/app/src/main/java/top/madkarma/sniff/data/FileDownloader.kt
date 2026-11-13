package top.madkarma.sniff.data

import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import top.madkarma.sniff.R
import top.madkarma.sniff.util.copyWithCancellation
import java.io.File

private const val PROGRESS_INTERVAL_NANOS = 200_000_000L

internal class FileDownloader(private val client: OkHttpClient) {
    suspend fun download(file: RemoteFile, output: File, onProgress: (Long, Long) -> Unit) {
        val url = file.url.toHttpUrl()
        check(url.isHttps) { "Download URL must use HTTPS" }

        val request = Request.Builder().url(url).build()

        client.newCall(request).await().use { response ->
            if (!response.isSuccessful) {
                throw AppException(uiText(R.string.error_file_http, file.label, response.code))
            }

            val body = response.body
            val total = body.contentLength()
            val partial = File(output.path + ".part")

            body.byteStream().use { input ->
                partial.outputStream().use { sink ->
                    var lastProgress = 0L
                    val copied = input.copyWithCancellation(sink) { bytes ->
                        val now = System.nanoTime()
                        if (now - lastProgress > PROGRESS_INTERVAL_NANOS) {
                            onProgress(bytes, total)
                            lastProgress = now
                        }
                    }

                    if (copied <= 0 || (total >= 0 && copied != total)) {
                        throw AppException(uiText(R.string.error_file_incomplete, file.label))
                    }

                    onProgress(copied, total)
                }
            }

            check(partial.renameTo(output)) { "Could not finish ${file.label}" }
        }
    }
}
