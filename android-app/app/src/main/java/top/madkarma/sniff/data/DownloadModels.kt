package top.madkarma.sniff.data

import java.io.File

data class DownloadResult(
    val apk: File,
    val folder: File,
    val fileCount: Int,
    val merged: Boolean,
    val title: String,
)

interface DownloadSource {
    suspend fun download(
        packageId: String,
        channel: ReleaseChannel,
        resign: Boolean = false,
        onProgress: (String) -> Unit,
    ): DownloadResult
}

enum class ReleaseChannel(val apiName: String) {
    STABLE("stable"), BETA("beta"), ALPHA("alpha"),
}

enum class ExportFormat(val mimeType: String) {
    APK("application/vnd.android.package-archive"), ZIP("application/zip");

    fun suggestedFileName(result: DownloadResult): String = when (this) {
        APK -> result.apk.name
        ZIP -> result.apk.name.removeSuffix(".apk") + "-files.zip"
    }
}
