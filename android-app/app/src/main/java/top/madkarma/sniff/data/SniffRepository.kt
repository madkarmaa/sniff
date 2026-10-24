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
        onProgress: (String) -> Unit,
    ): DownloadResult = withContext(io) {
        require(PACKAGE_ID_PATTERN.matches(packageId)) {
            "Enter a valid package ID, such as com.google.android.calculator"
        }

        onProgress("Fetching package information…")

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
        onProgress: (String) -> Unit,
    ): DownloadResult {
        var output = File(folder, cached.output)

        if (cached.resigned != (plan.hasSplits && resign) || !output.isFile) {
            // The cached downloads are intact, so only the output APK is rebuilt.
            val merged = File(folder, "merged.tmp")
            try {
                prepareApk(plan, folder, resign, onProgress, merged)
                output = merged.toPath().moveTo(apkFile(plan, folder).toPath(), overwrite = true)
                    .toFile()
            } finally {
                merged.delete()
            }
            writeInventory(plan, channel, folder, output, resign)
        }

        return DownloadResult(
            apk = output,
            folder = folder,
            fileCount = cached.files.size,
            merged = plan.hasSplits,
            title = plan.title,
            cached = true,
        )
    }

    private suspend fun downloadIntoCache(
        plan: DownloadPlan,
        channel: ReleaseChannel,
        folder: File,
        resign: Boolean,
        onProgress: (String) -> Unit,
    ): DownloadResult {
        // The new version is staged separately so a failed update keeps the cached one.
        val staging = createStagingFolder(plan.packageId)

        try {
            downloadFiles(plan, staging, onProgress)

            val output = prepareApk(plan, staging, resign, onProgress)

            currentCoroutineContext().ensureActive()

            writeInventory(plan, channel, staging, output, resign)

            folder.deleteRecursively()
            check(staging.renameTo(folder)) { "Could not update the cached download" }

            return DownloadResult(
                apk = File(folder, output.name),
                folder = folder,
                fileCount = plan.files.size,
                merged = plan.hasSplits,
                title = plan.title,
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
            CachedDownload(
                versionCode = json.getLong("version_code"),
                resigned = json.getBoolean("resigned"),
                files = List(files.length()) { files.getString(it) },
                output = json.getString("output"),
            )
        }.getOrNull() ?: return null

        val complete = cached.files.all { File(folder, it).isFile }
        return cached.takeIf { it.versionCode == plan.versionCode && complete }
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
        File(folder, "${plan.packageId}.apk")

    private suspend fun downloadFiles(
        plan: DownloadPlan, folder: File, onProgress: (String) -> Unit
    ) {
        plan.files.forEachIndexed { index, file ->
            currentCoroutineContext().ensureActive()

            val output = File(folder, file.relativePath)
            output.parentFile!!.mkdirs()

            downloader.download(file, output) { size, total ->
                val totalText = if (total > 0) " / ${formatBytes(total)}" else ""
                onProgress(
                    "Downloading ${index + 1}/${plan.files.size}: ${file.label}\n${formatBytes(size)}$totalText"
                )
            }
        }
    }

    private suspend fun prepareApk(
        plan: DownloadPlan,
        folder: File,
        resign: Boolean,
        onProgress: (String) -> Unit,
        output: File = apkFile(plan, folder),
    ): File {
        currentCoroutineContext().ensureActive()

        if (plan.hasSplits) {
            val action = if (resign) " and signing…" else "…"
            onProgress("Merging ${plan.splitCount + 1} APKs$action")
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
        plan: DownloadPlan, channel: ReleaseChannel, folder: File, output: File, resign: Boolean
    ) {
        // Signed download URLs expire and must not be included in exported inventories.
        val inventory =
            JSONObject().put("package", plan.packageId).put("version_code", plan.versionCode)
                .put("channel", channel.apiName).put("merged", plan.hasSplits)
                .put("resigned", plan.hasSplits && resign)
                .put("files", JSONArray(plan.files.map { it.relativePath }))
                .put("output", output.name)

        File(folder, INVENTORY_FILE).writeText(inventory.toString(2))
    }
}
