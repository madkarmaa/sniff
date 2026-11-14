package top.madkarma.sniff.ui

import android.content.Context
import android.content.Intent
import android.provider.Settings
import androidx.core.content.FileProvider
import androidx.core.net.toUri
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.ExportFormat

internal fun shareApk(context: Context, result: DownloadResult, title: String) {
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", result.apk)
    val intent =
        Intent(Intent.ACTION_SEND)
            .setType(ExportFormat.APK.mimeType)
            .putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    context.startActivity(Intent.createChooser(intent, title))
}

/** The system screen where the user allows this app to install packages. */
internal fun unknownSourcesSettings(context: Context): Intent =
    Intent(
        Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
        "package:${context.packageName}".toUri(),
    )
