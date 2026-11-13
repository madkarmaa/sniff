package top.madkarma.sniff.data

import java.io.File

data class DownloadResult(
    val apk: File,
    val folder: File,
    val fileCount: Int,
    val merged: Boolean,
    val title: String,
    val cached: Boolean = false,
    /** ABIs the server could not fetch, so the APK may lack native code for them. */
    val failedAbis: List<String> = emptyList(),
)

/** Reports what is happening; [fraction] is the overall progress from 0 to 1, or null when unknown. */
typealias ProgressListener = (message: UiText, fraction: Float?) -> Unit

interface DownloadSource {
    suspend fun download(
        packageId: String,
        channel: ReleaseChannel,
        resign: Boolean = false,
        onProgress: ProgressListener,
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
