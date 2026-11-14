package top.madkarma.sniff.service

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Process-wide flag that a download or install preparation is running. The ViewModel sets it; the
 * UI starts [DownloadService] when it turns true, and the service stops itself when it turns false.
 * The service cannot rely on the UI for stopping, because the UI stops observing while in the
 * background.
 */
internal object ActiveWork {
    private val running = MutableStateFlow(false)
    val active: StateFlow<Boolean> = running.asStateFlow()

    fun set(value: Boolean) {
        running.value = value
    }
}
