package top.madkarma.sniff.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import top.madkarma.sniff.R
import top.madkarma.sniff.data.ReleaseChannel

@Composable
internal fun DownloadForm(
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
        trailingIcon = if (packageId.isEmpty()) null else {
            {
                IconButton(onClick = { onPackageIdChange("") }, enabled = enabled) {
                    Icon(
                        painter = painterResource(R.drawable.ic_clear),
                        contentDescription = stringResource(R.string.clear),
                    )
                }
            }
        },
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
        modifier = Modifier
            .fillMaxWidth()
            .toggleable(
                value = resign,
                enabled = enabled,
                role = Role.Checkbox,
                onValueChange = onResignChange,
            ),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = resign, onCheckedChange = null, enabled = enabled)
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.resign))
            Text(
                text = stringResource(R.string.resign_description),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
    Button(
        onClick = onDownload,
        enabled = enabled && packageId.isNotBlank(),
        modifier = Modifier.fillMaxWidth(),
    ) { Text(stringResource(R.string.download_apk)) }
}

private fun ReleaseChannel.labelResource(): Int = when (this) {
    ReleaseChannel.STABLE -> R.string.stable
    ReleaseChannel.BETA -> R.string.beta
    ReleaseChannel.ALPHA -> R.string.alpha
}
