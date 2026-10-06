package top.madkarma.sniff.util

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.InputStream
import java.io.OutputStream

private const val COPY_BUFFER_SIZE = 64 * 1024

internal suspend fun InputStream.copyWithCancellation(
    output: OutputStream,
    onProgress: (Long) -> Unit = {},
): Long {
    val buffer = ByteArray(COPY_BUFFER_SIZE)
    var copied = 0L

    while (true) {
        currentCoroutineContext().ensureActive()

        val length = withContext(Dispatchers.IO) {
            read(buffer)
        }
        if (length == -1) return copied

        withContext(Dispatchers.IO) {
            output.write(buffer, 0, length)
        }

        copied += length
        onProgress(copied)
    }
}
