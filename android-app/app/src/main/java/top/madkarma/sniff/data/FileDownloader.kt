package top.madkarma.sniff.data

import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import top.madkarma.sniff.R
import top.madkarma.sniff.util.copyWithCancellation
import java.io.File
import java.security.DigestOutputStream
import java.security.MessageDigest

private const val PROGRESS_INTERVAL_NANOS = 200_000_000L

internal class FileDownloader(private val client: OkHttpClient) {
    /** Downloads [file] to [output], checks it against the size and hash the API gave, and returns its size. */
    suspend fun download(file: RemoteFile, output: File, onProgress: (Long, Long) -> Unit): Long {
        val url = file.url.toHttpUrl()
        check(url.isHttps) { "Download URL must use HTTPS" }

        val request = Request.Builder().url(url).build()

        return client.newCall(request).await().use { response ->
            if (!response.isSuccessful) {
                throw AppException(uiText(R.string.error_file_http, file.label, response.code))
            }

            val body = response.body
            val total = body.contentLength()
            val partial = File(output.path + ".part")
            // The hash is computed while the bytes are written, so verifying needs no second read.
            val digest = file.expectedHash()?.let { MessageDigest.getInstance(it.algorithm) }

            val copied = body.byteStream().use { input ->
                partial.outputStream().use { fileSink ->
                    val sink = if (digest == null) fileSink else DigestOutputStream(fileSink, digest)
                    var lastProgress = 0L
                    input.copyWithCancellation(sink) { bytes ->
                        val now = System.nanoTime()
                        if (now - lastProgress > PROGRESS_INTERVAL_NANOS) {
                            onProgress(bytes, total)
                            lastProgress = now
                        }
                    }
                }
            }

            try {
                if (copied <= 0 || (total >= 0 && copied != total)) {
                    throw AppException(uiText(R.string.error_file_incomplete, file.label))
                }
                verifyDownload(file, copied, digest?.digest())
            } catch (error: Throwable) {
                // A file that failed its check is discarded, not left for later use.
                partial.delete()
                throw error
            }

            onProgress(copied, total)

            check(partial.renameTo(output)) { "Could not finish ${file.label}" }
            copied
        }
    }
}
