package top.madkarma.sniff.ui

import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import top.madkarma.sniff.data.DownloadExporter
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.DownloadSource
import top.madkarma.sniff.data.ExportFormat
import top.madkarma.sniff.data.ReleaseChannel

data class DownloadUiState(
    val busy: Boolean = false,
    val message: String = "",
    val error: Boolean = false,
    val result: DownloadResult? = null,
)

class DownloadViewModel(
    private val repository: DownloadSource,
    private val exporter: DownloadExporter,
) : ViewModel() {
    private val mutableState = MutableStateFlow(DownloadUiState())
    val state: StateFlow<DownloadUiState> = mutableState.asStateFlow()
    private var task: Job? = null

    fun download(packageId: String, channel: ReleaseChannel, resign: Boolean = false) {
        if (state.value.busy) return
        mutableState.value = DownloadUiState(busy = true)

        task = viewModelScope.launch {
            try {
                val result = repository.download(packageId.trim(), channel, resign) { message ->
                    mutableState.update { it.copy(message = message) }
                }

                mutableState.value = DownloadUiState(
                    result = result,
                    message = "Ready · ${result.fileCount} files downloaded",
                )
            } catch (_: CancellationException) {
                mutableState.value =
                    DownloadUiState(message = "Cancelled. Temporary files removed.")
            } catch (error: Exception) {
                mutableState.value = DownloadUiState(
                    error = true,
                    message = error.message ?: "Download failed",
                )
            } finally {
                mutableState.update { it.copy(busy = false) }
            }
        }
    }

    fun cancel() {
        task?.cancel()
    }

    fun export(uri: Uri, result: DownloadResult, format: ExportFormat) {
        if (state.value.busy) return
        mutableState.update { it.copy(busy = true, error = false, message = "Saving…") }

        task = viewModelScope.launch {
            try {
                exporter.save(uri, result, format)
                mutableState.update { it.copy(message = "Saved successfully.") }
            } catch (_: CancellationException) {
                mutableState.update {
                    it.copy(message = "Save cancelled; the destination may contain a partial file.")
                }
            } catch (error: Exception) {
                mutableState.update {
                    it.copy(
                        error = true, message = error.message ?: "Save failed"
                    )
                }
            } finally {
                mutableState.update { it.copy(busy = false) }
            }
        }
    }
}
