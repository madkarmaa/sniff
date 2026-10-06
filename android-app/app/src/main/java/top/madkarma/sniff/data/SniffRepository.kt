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

private val PACKAGE_ID_PATTERN = Regex("[A-Za-z][A-Za-z0-9_]*(\\.[A-Za-z][A-Za-z0-9_]*)+")

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
        val folder = createTemporaryFolder(packageId)

        try {
            downloadFiles(plan, folder, onProgress)

            val output = prepareApk(plan, folder, resign, onProgress)

            currentCoroutineContext().ensureActive()

            writeInventory(plan, channel, folder, output, resign)

            DownloadResult(
                apk = output,
                folder = folder,
                fileCount = plan.files.size,
                merged = plan.hasSplits,
                title = plan.title,
            )
        } catch (error: Throwable) {
            folder.deleteRecursively()
            throw error
        }
    }

    private fun createTemporaryFolder(packageId: String): File {
        root.mkdirs()
        return File.createTempFile("$packageId-", "", root).apply {
            delete()
            check(mkdir())
        }
    }

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
    ): File {
        currentCoroutineContext().ensureActive()

        val output = File(folder, "${plan.packageId}.apk")

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

        File(folder, "inventory.json").writeText(inventory.toString(2))
    }
}
