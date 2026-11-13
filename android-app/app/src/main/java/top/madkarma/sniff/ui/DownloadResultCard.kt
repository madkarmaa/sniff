package top.madkarma.sniff.ui

import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import top.madkarma.sniff.R
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.ExportFormat
import top.madkarma.sniff.util.formatBytes

@Composable
internal fun DownloadResultCard(
    result: DownloadResult,
    enabled: Boolean,
    onInstall: () -> Unit,
    onExport: (ExportFormat) -> Unit,
    onShare: () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(result.title, style = MaterialTheme.typography.titleLarge)
            Text("${result.apk.name} · ${formatBytes(result.apk.length())}")
            // Not an error. The download worked, but some device variants are missing from it.
            // Only ABIs this device can run matter here; the others would go unused anyway.
            val missingAbis = result.failedAbis.filter { it in Build.SUPPORTED_ABIS }
            if (missingAbis.isNotEmpty()) {
                Text(
                    text = stringResource(
                        R.string.note_partial_targets, missingAbis.joinToString(", ")
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Button(
                onClick = onInstall,
                enabled = enabled,
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.install_apk)) }
            // Only a merged APK differs from the installed files.
            if (result.merged) {
                Text(
                    text = stringResource(R.string.install_hint),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            OutlinedButton(
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
