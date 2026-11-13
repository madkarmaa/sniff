package top.madkarma.sniff.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import top.madkarma.sniff.R
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.service.ActiveWork
import top.madkarma.sniff.service.startDownloadService

/** Text from outside the app for the package field; each instance is a separate event. */
internal class IncomingInput(val text: String)

@Composable
internal fun DownloadScreen(model: DownloadViewModel, incoming: IncomingInput? = null) {
    val state by model.state.collectAsStateWithLifecycle()
    var packageId by rememberSaveable { mutableStateOf("") }
    // Only prefills; the user starts the download.
    LaunchedEffect(incoming) { if (incoming != null) packageId = incoming.text }
    var channel by rememberSaveable { mutableStateOf(ReleaseChannel.STABLE) }
    var resign by rememberSaveable { mutableStateOf(false) }
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    val shareTitle = stringResource(R.string.share_apk)

    // The foreground service keeps the process alive in the background; it stops itself when the
    // work ends, so only the start is requested here. Starting is only allowed in the foreground.
    val working by ActiveWork.active.collectAsStateWithLifecycle()
    LaunchedEffect(working) { if (working) startDownloadService(context) }

    val askNotifications = rememberNotificationRequest()
    val currentResult = { state.result }
    val install = rememberInstallAction(model, currentResult)
    val export = rememberExportAction(model, currentResult)

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
                askNotifications()
                model.download(packageId, channel, resign)
            },
        )
        DownloadStatus(state = state, onCancel = model::cancel)
        state.result?.let { result ->
            DownloadResultCard(
                result = result,
                enabled = !state.busy,
                onInstall = { install(result) },
                onExport = { format -> export(result, format) },
                onShare = { if (model.isCached(result)) shareApk(context, result, shareTitle) },
            )
        }
    }
}
