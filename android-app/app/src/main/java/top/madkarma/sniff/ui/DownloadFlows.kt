package top.madkarma.sniff.ui

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.ExportFormat

/**
 * Returns the request to make before the first download. It is asked once, and the download does
 * not wait for the answer. Without the permission the service still runs and only its
 * notification stays hidden.
 */
@Composable
internal fun rememberNotificationRequest(): () -> Unit {
    val context = LocalContext.current
    var askedForNotifications by rememberSaveable { mutableStateOf(false) }
    val askNotifications = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) {}

    return {
        // Notification permission exists from Android 13 (API 33).
        if (!askedForNotifications && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(
                context, Manifest.permission.POST_NOTIFICATIONS
            ) != PackageManager.PERMISSION_GRANTED
        ) {
            askedForNotifications = true
            askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}

/**
 * Returns the install action. It asks the user to allow installs from this app when that is
 * needed; [currentResult] reads the result shown right now.
 */
@Composable
internal fun rememberInstallAction(
    model: DownloadViewModel,
    currentResult: () -> DownloadResult?,
): (DownloadResult) -> Unit {
    val context = LocalContext.current
    var pendingInstall by remember { mutableStateOf<DownloadResult?>(null) }

    // Returning from the settings screen resumes the install if the user allowed it there.
    val allowInstalls = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) {
        val result = pendingInstall ?: currentResult()
        if (result != null && context.packageManager.canRequestPackageInstalls()) {
            model.install(result)
        }
    }

    return install@{ result ->
        if (!model.isCached(result)) return@install

        if (context.packageManager.canRequestPackageInstalls()) {
            model.install(result)
            return@install
        }

        pendingInstall = result
        model.showInstallPermissionNeeded()
        try {
            allowInstalls.launch(unknownSourcesSettings(context))
        } catch (_: ActivityNotFoundException) {
            // The message above still tells the user where to look.
        }
    }
}

/** Returns the export action, which lets the user pick where the APK or ZIP is saved. */
@Composable
internal fun rememberExportAction(
    model: DownloadViewModel,
    currentResult: () -> DownloadResult?,
): (DownloadResult, ExportFormat) -> Unit {
    var pendingExport by remember { mutableStateOf<DownloadResult?>(null) }

    fun export(uri: Uri?, format: ExportFormat) {
        val result = pendingExport ?: currentResult()
        if (uri != null && result != null) model.export(uri, result, format)
    }

    val saveApk = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument(ExportFormat.APK.mimeType)
    ) { export(it, ExportFormat.APK) }
    val saveZip = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument(ExportFormat.ZIP.mimeType)
    ) { export(it, ExportFormat.ZIP) }

    return { result, format ->
        if (model.isCached(result)) {
            pendingExport = result
            val fileName = format.suggestedFileName(result)

            when (format) {
                ExportFormat.APK -> saveApk.launch(fileName)
                ExportFormat.ZIP -> saveZip.launch(fileName)
            }
        }
    }
}
