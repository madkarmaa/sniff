package top.madkarma.sniff.data

import android.content.Context
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import top.madkarma.sniff.R
import java.io.File
import kotlin.io.path.createTempDirectory
import kotlin.io.path.moveTo

private val PACKAGE_ID_PATTERN = Regex("[A-Za-z][A-Za-z0-9_]*(\\.[A-Za-z][A-Za-z0-9_]*)+")

class SniffRepository(
    context: Context,
    client: OkHttpClient = createHttpClient(),
    private val io: CoroutineDispatcher = Dispatchers.IO,
    /** Read for every lookup, so a changed server applies to the next download. */
    serverUrl: () -> String = { DEFAULT_SERVER_URL },
) : DownloadSource {
    private val root = File(context.cacheDir, "downloads")
    private val api = SniffApi(client, serverUrl)
    private val downloader = FileDownloader(client)

    override suspend fun download(
        packageId: String,
        channel: ReleaseChannel,
        resign: Boolean,
        onProgress: ProgressListener,
    ): DownloadResult = withContext(io) {
        if (!PACKAGE_ID_PATTERN.matches(packageId)) {
            throw AppException(uiText(R.string.error_invalid_package))
        }

        onProgress(uiText(R.string.progress_fetching), null)

        val plan = api.fetchDownloadPlan(packageId, channel)
        val folder = File(root, packageId)
        removeIncompleteFolders()

        val cached = readCachedDownload(folder, plan)
        if (cached != null) {
            useCachedDownload(plan, channel, folder, cached, resign, onProgress)
        } else {
            downloadIntoCache(plan, channel, folder, resign, onProgress)
        }
    }

    private suspend fun useCachedDownload(
        plan: DownloadPlan,
        channel: ReleaseChannel,
        folder: File,
        cached: CachedDownload,
        resign: Boolean,
        onProgress: ProgressListener,
    ): DownloadResult {
        val output = apkFile(plan, folder)
        val previous = File(folder, cached.output)

        // Older app versions named the APK without its version code.
        val renamed = previous != output
        if (renamed && !previous.renameTo(output)) previous.delete()

        val rebuilt = cached.resigned != (plan.hasSplits && resign) || !output.isFile
        if (rebuilt) {
            // The cached downloads are intact, so only the output APK is rebuilt.
            val merged = File(folder, "merged.tmp")
            try {
                prepareApk(plan, folder, resign, onProgress, merged)
                merged.toPath().moveTo(output.toPath(), overwrite = true)
            } finally {
                merged.delete()
            }
        }

        if (renamed || rebuilt) {
            writeInventory(plan, channel, folder, output, resign, cached.failedAbis)
        }

        return DownloadResult(
            apk = output,
            folder = folder,
            fileCount = cached.files.size,
            merged = plan.hasSplits,
            title = plan.title,
            cached = true,
            failedAbis = cached.failedAbis,
        )
    }

    private suspend fun downloadIntoCache(
        plan: DownloadPlan,
        channel: ReleaseChannel,
        folder: File,
        resign: Boolean,
        onProgress: ProgressListener,
    ): DownloadResult {
        // The new version is staged separately so a failed update keeps the cached one.
        val staging = createStagingFolder(plan.packageId)

        try {
            downloadFiles(downloader, plan, staging, onProgress)

            val output = prepareApk(plan, staging, resign, onProgress)

            currentCoroutineContext().ensureActive()

            writeInventory(plan, channel, staging, output, resign, plan.failedAbis)

            folder.deleteRecursively()
            check(staging.renameTo(folder)) { "Could not update the cached download" }

            return DownloadResult(
                apk = File(folder, output.name),
                folder = folder,
                fileCount = plan.files.size,
                merged = plan.hasSplits,
                title = plan.title,
                failedAbis = plan.failedAbis,
            )
        } catch (error: Throwable) {
            staging.deleteRecursively()
            throw error
        }
    }

    /** Staging folders and folders from older app versions are never valid package IDs. */
    private fun removeIncompleteFolders() {
        root.listFiles { file -> !PACKAGE_ID_PATTERN.matches(file.name) }
            ?.forEach { it.deleteRecursively() }
    }

    private fun createStagingFolder(packageId: String): File {
        root.mkdirs()
        return createTempDirectory(root.toPath(), "$packageId-").toFile()
    }
}
