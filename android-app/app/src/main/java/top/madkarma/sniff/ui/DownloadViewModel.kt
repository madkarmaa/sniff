package top.madkarma.sniff.ui

import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import top.madkarma.sniff.R
import top.madkarma.sniff.apk.InstallEvents
import top.madkarma.sniff.apk.SplitInstaller
import top.madkarma.sniff.data.DownloadExporter
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.DownloadSource
import top.madkarma.sniff.data.ExportFormat
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.data.packageIdFromInput
import top.madkarma.sniff.data.toDownloadError
import top.madkarma.sniff.data.toSaveError
import top.madkarma.sniff.data.uiText
import top.madkarma.sniff.service.ActiveWork

class DownloadViewModel(
    private val repository: DownloadSource,
    private val exporter: DownloadExporter,
    private val installer: SplitInstaller,
) : ViewModel() {
    private val mutableState = MutableStateFlow(DownloadUiState())
    val state: StateFlow<DownloadUiState> = mutableState.asStateFlow()
    private var task: Job? = null
    private val installs = InstallTracker(installer, mutableState)

    init {
        // Outcomes are collected for the whole life of the ViewModel, also those of sessions this
        // instance did not start, such as one finished after the process was recreated.
        viewModelScope.launch {
            InstallEvents.outcomes.filterNotNull().collect { installs.onOutcome(it) }
        }
    }

    /** Downloads the app named by [input], either a package ID or a Google Play link. */
    fun download(input: String, channel: ReleaseChannel, resign: Boolean = false) {
        if (state.value.busy) return
        mutableState.value = DownloadUiState(busy = true)

        task = launchWork {
            try {
                val packageId = packageIdFromInput(input)
                val result =
                    repository.download(packageId, channel, resign) { message, fraction ->
                        mutableState.update { it.copy(message = message, progress = fraction) }
                    }

                mutableState.value =
                    DownloadUiState(
                        result = result,
                        message =
                            if (result.cached) {
                                uiText(R.string.status_ready_cached)
                            } else {
                                uiText(R.string.status_ready, result.fileCount)
                            },
                    )
            } catch (_: CancellationException) {
                mutableState.value = DownloadUiState(message = uiText(R.string.status_cancelled))
            } catch (error: Exception) {
                mutableState.value =
                    DownloadUiState(error = true, message = error.toDownloadError())
            } finally {
                mutableState.update { it.copy(busy = false) }
            }
        }
    }

    /** Returns false, and drops the result, when the system cleared its cached files. */
    fun isCached(result: DownloadResult): Boolean {
        if (result.apk.isFile) return true

        mutableState.value =
            DownloadUiState(error = true, message = uiText(R.string.error_cache_removed))

        return false
    }

    fun cancel() {
        // Once the session is committed the task is done and the system dialog cannot be recalled.
        // Cancelling then only stops waiting; a later outcome is still shown.
        if (installs.isAwaiting && task?.isActive != true) installs.stopAwaiting()
        else task?.cancel()
    }

    /** Asks the user to allow installs from this app; the UI opens the settings screen for it. */
    fun showInstallPermissionNeeded() {
        mutableState.update {
            it.copy(error = false, message = uiText(R.string.error_install_unknown_sources))
        }
    }

    /** Installs the original APK files of [result], see [InstallTracker.install]. */
    fun install(result: DownloadResult) {
        if (state.value.busy) return
        mutableState.update {
            it.copy(busy = true, error = false, message = uiText(R.string.status_install_preparing))
        }

        task = launchWork { installs.install(result) }
    }

    fun export(uri: Uri, result: DownloadResult, format: ExportFormat) {
        if (state.value.busy) return
        mutableState.update {
            it.copy(busy = true, error = false, message = uiText(R.string.status_saving))
        }

        task = viewModelScope.launch {
            try {
                exporter.save(uri, result, format)
                mutableState.update { it.copy(message = uiText(R.string.status_saved)) }
            } catch (_: CancellationException) {
                mutableState.update { it.copy(message = uiText(R.string.status_save_cancelled)) }
            } catch (error: Exception) {
                mutableState.update { it.copy(error = true, message = error.toSaveError()) }
            } finally {
                mutableState.update { it.copy(busy = false) }
            }
        }
    }

    /**
     * Runs [block] as the current task. While it runs the process counts as busy, which keeps the
     * foreground service alive, see [ActiveWork]. The install task ends once the session is
     * committed, so waiting for the system dialog does not hold the service.
     */
    private fun launchWork(block: suspend () -> Unit): Job {
        ActiveWork.set(true)
        return viewModelScope
            .launch { block() }
            .also {
                it.invokeOnCompletion { ActiveWork.set(false) }
            }
    }

    override fun onCleared() {
        // The scope is cancelled, so the task is too; this also covers a task that never started.
        ActiveWork.set(false)
    }
}
