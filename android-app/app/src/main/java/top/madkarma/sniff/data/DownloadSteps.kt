package top.madkarma.sniff.data

import com.reandroid.apk.ApkModule
import java.io.File
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import top.madkarma.sniff.R
import top.madkarma.sniff.apk.ApkMerger
import top.madkarma.sniff.util.formatBytes

internal fun apkFile(plan: DownloadPlan, folder: File): File =
    File(folder, "${plan.packageId}_${plan.versionCode}.apk")

internal suspend fun downloadFiles(
    downloader: FileDownloader,
    plan: DownloadPlan,
    folder: File,
    onProgress: ProgressListener,
) {
    // Bytes of the files that are done. Together with the size of the current file and the API
    // sizes of the later ones this gives the overall total; any later file of unknown size
    // makes the total, and so the progress bar, indeterminate.
    var finished = 0L

    plan.files.forEachIndexed { index, file ->
        currentCoroutineContext().ensureActive()

        val output = File(folder, file.relativePath)
        output.parentFile!!.mkdirs()

        val laterSizes = plan.files.drop(index + 1).map { it.size }
        val laterTotal = if (null in laterSizes) null else laterSizes.sumOf { it ?: 0L }

        val downloaded =
            downloader.download(file, output) { received, contentLength ->
                // The API size is exact; otherwise the response header is the best guess.
                val expected = file.size ?: contentLength.takeIf { it > 0 }
                val total =
                    if (expected != null && laterTotal != null) finished + expected + laterTotal
                    else null

                val sizeText =
                    if (expected != null) {
                        "${formatBytes(received)} / ${formatBytes(expected)}"
                    } else {
                        formatBytes(received)
                    }
                onProgress(
                    uiText(
                        R.string.progress_downloading,
                        index + 1,
                        plan.files.size,
                        file.label,
                        sizeText,
                    ),
                    total?.let { ((finished + received).toFloat() / it).coerceIn(0f, 1f) },
                )
            }
        finished += downloaded
    }
}

internal suspend fun prepareApk(
    plan: DownloadPlan,
    folder: File,
    resign: Boolean,
    onProgress: ProgressListener,
    output: File = apkFile(plan, folder),
): File {
    currentCoroutineContext().ensureActive()

    if (plan.hasSplits) {
        val message = if (resign) R.string.progress_merging_signing else R.string.progress_merging
        onProgress(uiText(message, plan.splitCount + 1), null)
        ApkMerger.merge(File(folder, "apks"), output, plan.packageId, plan.versionCode, resign)
    } else {
        copyOriginalApk(File(folder, "apks/base.apk"), output, plan)
    }

    return output
}

private fun copyOriginalApk(input: File, output: File, plan: DownloadPlan) {
    ApkModule.loadApkFile(input).use { apk ->
        val version = apk.androidManifest.versionCode?.toLong()?.and(0xffffffffL)
        check(apk.packageName == plan.packageId && version == plan.versionCode) {
            "Downloaded APK does not match the package/version"
        }
    }

    input.copyTo(output)
}
