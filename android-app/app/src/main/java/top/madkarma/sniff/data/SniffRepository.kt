package top.madkarma.sniff.data

import android.content.Context
import com.reandroid.apk.ApkModule
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import org.json.JSONArray
import org.json.JSONObject
import top.madkarma.sniff.R
import top.madkarma.sniff.apk.ApkMerger
import top.madkarma.sniff.util.formatBytes
import java.io.File
import kotlin.io.path.createTempDirectory
import kotlin.io.path.moveTo

private val PACKAGE_ID_PATTERN = Regex("[A-Za-z][A-Za-z0-9_]*(\\.[A-Za-z][A-Za-z0-9_]*)+")
private const val INVENTORY_FILE = "inventory.json"

/** The inventory of a complete cached download; it is written last, so its presence marks completion. */
private data class CachedDownload(
    val versionCode: Long,
    val resigned: Boolean,
    val files: List<String>,
    val output: String,
    /** The ABIs this download lacks because the server could not fetch them. */
    val failedAbis: List<String>,
)

class SniffRepository(
    context: Context,
    client: OkHttpClient = createHttpClient(),
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : DownloadSource {
    private val root = File(context.cacheDir, "downloads")
    private val api = SniffApi(client)
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
            downloadFiles(plan, staging, onProgress)

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

    private fun readCachedDownload(folder: File, plan: DownloadPlan): CachedDownload? {
        val inventory = File(folder, INVENTORY_FILE)
        if (!inventory.isFile) return null

        val cached = runCatching {
            val json = JSONObject(inventory.readText())
            val files = json.getJSONArray("files")
            // Inventories written before API v3 have no list; they were complete downloads.
            val failed = json.optJSONArray("failed_abis")
            CachedDownload(
                versionCode = json.getLong("version_code"),
                resigned = json.getBoolean("resigned"),
                files = List(files.length()) { files.getString(it) },
                output = json.getString("output"),
                failedAbis = failed?.let { abis -> List(abis.length()) { abis.getString(it) } }.orEmpty(),
            )
        }.getOrNull() ?: return null

        val complete = cached.files.all { File(folder, it).isFile }
        // A download that lacks ABIs is only reused if the server still cannot offer them; otherwise
        // downloading again completes it.
        val sufficient = plan.failedAbis.containsAll(cached.failedAbis)
        return cached.takeIf { it.versionCode == plan.versionCode && complete && sufficient }
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

    private fun apkFile(plan: DownloadPlan, folder: File): File =
        File(folder, "${plan.packageId}_${plan.versionCode}.apk")

    private suspend fun downloadFiles(plan: DownloadPlan, folder: File, onProgress: ProgressListener) {
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

            val downloaded = downloader.download(file, output) { received, contentLength ->
                // The API size is exact; otherwise the response header is the best guess.
                val expected = file.size ?: contentLength.takeIf { it > 0 }
                val total =
                    if (expected != null && laterTotal != null) finished + expected + laterTotal else null

                val sizeText = if (expected != null) {
                    "${formatBytes(received)} / ${formatBytes(expected)}"
                } else {
                    formatBytes(received)
                }
                onProgress(
                    uiText(
                        R.string.progress_downloading, index + 1, plan.files.size, file.label, sizeText
                    ),
                    total?.let { ((finished + received).toFloat() / it).coerceIn(0f, 1f) },
                )
            }
            finished += downloaded
        }
    }

    private suspend fun prepareApk(
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

    private fun writeInventory(
        plan: DownloadPlan,
        channel: ReleaseChannel,
        folder: File,
        output: File,
        resign: Boolean,
        failedAbis: List<String>,
    ) {
        // Signed download URLs expire and must not be included in exported inventories.
        val inventory =
            JSONObject().put("package", plan.packageId).put("version_code", plan.versionCode)
                .put("channel", channel.apiName).put("merged", plan.hasSplits)
                .put("resigned", plan.hasSplits && resign)
                .put("files", JSONArray(plan.files.map { it.relativePath }))
                .put("output", output.name).put("failed_abis", JSONArray(failedAbis))

        File(folder, INVENTORY_FILE).writeText(inventory.toString(2))
    }
}
