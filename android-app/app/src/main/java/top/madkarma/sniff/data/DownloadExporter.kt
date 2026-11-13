package top.madkarma.sniff.data

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import top.madkarma.sniff.R
import top.madkarma.sniff.util.copyWithCancellation
import java.io.OutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class DownloadExporter(
    private val resolver: ContentResolver,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    suspend fun save(uri: Uri, result: DownloadResult, format: ExportFormat): Unit =
        withContext(io) {
            try {
                if (!result.apk.isFile) throw AppException(uiText(R.string.error_cache_removed))

                val stream = resolver.openOutputStream(uri)
                    ?: throw AppException(uiText(R.string.error_open_destination))
                stream.use { output ->
                    when (format) {
                        ExportFormat.APK -> result.apk.inputStream()
                            .use { it.copyWithCancellation(output) }

                        ExportFormat.ZIP -> writeBundle(result, output)
                    }
                }
            } catch (error: Throwable) {
                // The document picker already created the destination; do not leave it empty or partial.
                runCatching { DocumentsContract.deleteDocument(resolver, uri) }
                throw error
            }
        }

    private suspend fun writeBundle(result: DownloadResult, output: OutputStream) {
        ZipOutputStream(output).use { zip ->
            result.folder.walkTopDown().filter { it.isFile }.forEach { file ->
                currentCoroutineContext().ensureActive()

                val relativePath = file.relativeTo(result.folder).invariantSeparatorsPath

                zip.putNextEntry(ZipEntry(relativePath))
                file.inputStream().use { it.copyWithCancellation(zip) }
                zip.closeEntry()
            }
        }
    }
}
