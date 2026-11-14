package top.madkarma.sniff.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import top.madkarma.sniff.R
import top.madkarma.sniff.data.DEFAULT_SERVER_URL
import top.madkarma.sniff.data.ServerSettings
import top.madkarma.sniff.data.normalizeServerUrl

/** The settings icon of the screen header; it opens [ServerDialog]. */
@Composable
internal fun ServerSettingsButton(serverSettings: ServerSettings) {
    var editing by rememberSaveable { mutableStateOf(false) }

    IconButton(onClick = { editing = true }) {
        Icon(
            painter = painterResource(R.drawable.ic_settings),
            contentDescription = stringResource(R.string.server_settings),
        )
    }

    if (editing) {
        ServerDialog(
            currentUrl = serverSettings.serverUrl,
            onSave = {
                serverSettings.serverUrl = it
                editing = false
            },
            onDismiss = { editing = false },
        )
    }
}

/** Lets the user point the app at another Sniff server; [onSave] receives the normalized URL. */
@Composable
private fun ServerDialog(
    currentUrl: String,
    onSave: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var input by rememberSaveable { mutableStateOf(currentUrl) }
    val serverUrl = normalizeServerUrl(input)

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.server_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.server_description))
                OutlinedTextField(
                    value = input,
                    onValueChange = { input = it },
                    label = { Text(stringResource(R.string.server_url)) },
                    singleLine = true,
                    isError = serverUrl == null,
                    supportingText =
                        if (serverUrl != null) null
                        else {
                            { Text(stringResource(R.string.error_server_url, DEFAULT_SERVER_URL)) }
                        },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { serverUrl?.let(onSave) }, enabled = serverUrl != null) {
                Text(stringResource(R.string.save))
            }
        },
        dismissButton = {
            Row {
                TextButton(onClick = { input = DEFAULT_SERVER_URL }) {
                    Text(stringResource(R.string.server_reset))
                }
                TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
            }
        },
    )
}
