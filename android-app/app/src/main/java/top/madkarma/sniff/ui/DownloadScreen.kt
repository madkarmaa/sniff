package top.madkarma.sniff.ui

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Checkbox
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import top.madkarma.sniff.R
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.ExportFormat
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.util.formatBytes

@Composable
internal fun DownloadScreen(model: DownloadViewModel) {
    val state by model.state.collectAsStateWithLifecycle()
    var packageId by rememberSaveable { mutableStateOf("") }
    var channel by rememberSaveable { mutableStateOf(ReleaseChannel.STABLE) }
    var resign by rememberSaveable { mutableStateOf(false) }
    var pendingExport by remember { mutableStateOf<DownloadResult?>(null) }
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    val shareTitle = stringResource(R.string.share_apk)

    fun export(uri: Uri?, format: ExportFormat) {
        val result = pendingExport ?: state.result
        if (uri != null && result != null) model.export(uri, result, format)
    }

    val saveApk = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument(ExportFormat.APK.mimeType)
    ) { export(it, ExportFormat.APK) }
    val saveZip = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument(ExportFormat.ZIP.mimeType)
    ) { export(it, ExportFormat.ZIP) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .safeDrawingPadding()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(stringResource(R.string.app_name), style = MaterialTheme.typography.headlineLarge)
        Text(stringResource(R.string.introduction))
        DownloadForm(
            packageId = packageId,
            channel = channel,
            resign = resign,
            enabled = !state.busy,
            onPackageIdChange = { packageId = it },
            onChannelChange = { channel = it },
            onResignChange = { resign = it },
            onDownload = {
                focus.clearFocus()
                model.download(packageId, channel, resign)
            },
        )
        DownloadStatus(state = state, onCancel = model::cancel)
        state.result?.let { result ->
            DownloadResultCard(
                result = result,
                enabled = !state.busy,
                onExport = { format ->
                    pendingExport = result
                    val fileName = format.suggestedFileName(result)
                    when (format) {
                        ExportFormat.APK -> saveApk.launch(fileName)
                        ExportFormat.ZIP -> saveZip.launch(fileName)
                    }
                },
                onShare = { shareApk(context, result, shareTitle) },
            )
        }
    }
}

@Composable
private fun DownloadForm(
    packageId: String,
    channel: ReleaseChannel,
    resign: Boolean,
    enabled: Boolean,
    onPackageIdChange: (String) -> Unit,
    onChannelChange: (ReleaseChannel) -> Unit,
    onResignChange: (Boolean) -> Unit,
    onDownload: () -> Unit,
) {
    OutlinedTextField(
        value = packageId,
        onValueChange = onPackageIdChange,
        enabled = enabled,
        label = { Text(stringResource(R.string.package_id)) },
        placeholder = { Text(stringResource(R.string.package_example)) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii),
        modifier = Modifier.fillMaxWidth(),
    )
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        ReleaseChannel.entries.forEach { choice ->
            FilterChip(
                selected = channel == choice,
                enabled = enabled,
                onClick = { onChannelChange(choice) },
                label = { Text(stringResource(choice.labelResource())) },
            )
        }
    }
    Row(
        modifier = Modifier.fillMaxWidth().toggleable(
            value = resign,
            enabled = enabled,
            role = Role.Checkbox,
            onValueChange = onResignChange,
        ),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = resign, onCheckedChange = null, enabled = enabled)
        Text(stringResource(R.string.resign))
    }
    Button(
        onClick = onDownload,
        enabled = enabled && packageId.isNotBlank(),
        modifier = Modifier.fillMaxWidth(),
    ) { Text(stringResource(R.string.download_apk)) }
}

@Composable
private fun DownloadStatus(state: DownloadUiState, onCancel: () -> Unit) {
    if (state.busy) {
        LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
        OutlinedButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
    }
    if (state.message.isNotBlank()) {
        Text(
            text = state.message,
            color = if (state.error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
            style = MaterialTheme.typography.bodyLarge,
        )
    }
}

@Composable
private fun DownloadResultCard(
    result: DownloadResult,
    enabled: Boolean,
    onExport: (ExportFormat) -> Unit,
    onShare: () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(result.title, style = MaterialTheme.typography.titleLarge)
            Text("${result.apk.name} · ${formatBytes(result.apk.length())}")
            Button(
                onClick = { onExport(ExportFormat.APK) },
                enabled = enabled,
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.save_apk)) }
            OutlinedButton(
                onClick = onShare,
                enabled = enabled,
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.share_apk)) }
            OutlinedButton(
                onClick = { onExport(ExportFormat.ZIP) },
                enabled = enabled,
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.save_zip)) }
        }
    }
}

private fun ReleaseChannel.labelResource(): Int = when (this) {
    ReleaseChannel.STABLE -> R.string.stable
    ReleaseChannel.BETA -> R.string.beta
    ReleaseChannel.ALPHA -> R.string.alpha
}

private fun shareApk(context: Context, result: DownloadResult, title: String) {
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", result.apk)
    val intent = Intent(Intent.ACTION_SEND).setType(ExportFormat.APK.mimeType)
        .putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    context.startActivity(Intent.createChooser(intent, title))
}
