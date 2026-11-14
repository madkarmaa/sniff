package top.madkarma.sniff.apk

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.IntentSender
import android.content.pm.PackageInstaller
import android.os.Build
import androidx.core.app.PendingIntentCompat
import java.io.File
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import top.madkarma.sniff.R
import top.madkarma.sniff.data.AppException
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.uiText
import top.madkarma.sniff.util.copyWithCancellation

private const val BASE_APK = "base.apk"
private val KNOWN_ABIS =
    setOf("arm64_v8a", "armeabi_v7a", "armeabi", "x86", "x86_64", "mips", "mips64", "riscv64")
private val SPLIT_NUMBER_PREFIX = Regex("^split-\\d+-")
private val ABI_SPLIT = Regex("(?:^|[._])config\\.([A-Za-z0-9_-]+)$")

private fun String.normalizedAbi(): String = lowercase().replace('-', '_')

/** The ABI of a split file such as split-0003-config.arm64_v8a.apk, or null for any other split. */
private fun splitAbi(fileName: String, knownAbis: Set<String>): String? {
    val name = fileName.removeSuffix(".apk").replace(SPLIT_NUMBER_PREFIX, "")
    val abi = ABI_SPLIT.find(name)?.groupValues?.get(1)?.normalizedAbi() ?: return null
    return abi.takeIf { it in knownAbis }
}

/**
 * Picks the APK files to install from the file names of a download: everything except the native
 * library splits of ABIs other than the best one. The best ABI is the first of [supportedAbis] (in
 * the preference order of Build.SUPPORTED_ABIS) that has a split; without one, all ABI splits go.
 */
internal fun selectInstallFiles(
    fileNames: List<String>,
    supportedAbis: List<String>,
): List<String> {
    val preferred = supportedAbis.map { it.normalizedAbi() }
    val abiOf = fileNames.associateWith { splitAbi(it, KNOWN_ABIS + preferred) }
    val best = preferred.firstOrNull { it in abiOf.values }

    return fileNames.filter { name -> abiOf[name].let { it == null || it == best } }
}

/** Expansion files are not part of a package install; the user has to copy them. */
internal fun DownloadResult.hasExpansionFiles(): Boolean =
    File(folder, "additional").listFiles()?.any { it.extension == "obb" } == true

/**
 * Installs the original, developer-signed base and split APKs of a download with a PackageInstaller
 * session. Unlike the merged APK, this can update a copy installed from Google Play.
 */
class SplitInstaller(
    private val context: Context,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    /**
     * Stages the APK files in a session and commits it. Returns the session ID the outcome is
     * reported under, see [InstallEvents]. [onSession] runs before the commit, so the caller knows
     * the ID before any outcome can arrive. Cancelling before the commit abandons the session.
     */
    suspend fun install(result: DownloadResult, onSession: (Int) -> Unit = {}): Int =
        withContext(io) {
            val files = installFiles(result)
            val installer = context.packageManager.packageInstaller
            val params =
                PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)

            // The folder is named after the package ID; the system rejects files of another
            // package.
            params.setAppPackageName(result.folder.name)
            params.setSize(files.sumOf { it.length() })

            val sessionId = installer.createSession(params)
            onSession(sessionId)

            var committed = false
            try {
                installer.openSession(sessionId).use { session ->
                    files.forEach { stage(session, it) }
                    session.commit(statusReceiver(sessionId))
                    committed = true
                }
            } catch (error: Throwable) {
                if (!committed) runCatching { installer.abandonSession(sessionId) }
                throw error
            }

            sessionId
        }

    private fun installFiles(result: DownloadResult): List<File> {
        val folder = File(result.folder, "apks")
        val names = (folder.list() ?: emptyArray()).filter { it.endsWith(".apk") }.sorted()
        if (BASE_APK !in names) throw AppException(uiText(R.string.error_cache_removed))

        return selectInstallFiles(names, Build.SUPPORTED_ABIS.toList()).map { File(folder, it) }
    }

    private suspend fun stage(session: PackageInstaller.Session, file: File) {
        file.inputStream().use { input ->
            session.openWrite(file.name, 0, file.length()).use { output ->
                input.copyWithCancellation(output)
                session.fsync(output)
            }
        }
    }

    /**
     * The system adds the status to the intent, so the PendingIntent has to be mutable (API 31+).
     */
    private fun statusReceiver(sessionId: Int): IntentSender {
        val intent = Intent(context, InstallResultReceiver::class.java)
        val pending =
            PendingIntentCompat.getBroadcast(
                context,
                sessionId,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT,
                true,
            )
        return checkNotNull(pending).intentSender
    }
}
