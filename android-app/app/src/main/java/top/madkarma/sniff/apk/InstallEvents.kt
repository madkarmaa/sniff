package top.madkarma.sniff.apk

import android.content.pm.PackageInstaller
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import top.madkarma.sniff.R
import top.madkarma.sniff.data.UiText
import top.madkarma.sniff.data.uiText

/** The final result of an install session, as reported by the system. */
internal data class InstallOutcome(val sessionId: Int, val status: Int, val text: UiText) {
    val success: Boolean
        get() = status == PackageInstaller.STATUS_SUCCESS

    /** A cancel by the user is not an error. */
    val error: Boolean
        get() = !success && status != PackageInstaller.STATUS_FAILURE_ABORTED
}

/**
 * Process-wide channel from [InstallResultReceiver] to the ViewModel. It holds the latest outcome
 * until a collector consumes it, so a result that arrives while no ViewModel exists, for example
 * after the process was killed during the system dialog, is shown once the UI is back.
 */
internal object InstallEvents {
    private val latest = MutableStateFlow<InstallOutcome?>(null)
    val outcomes: StateFlow<InstallOutcome?> = latest.asStateFlow()

    fun publish(outcome: InstallOutcome) {
        latest.value = outcome
    }

    /** Clears [outcome] unless a newer one has replaced it. */
    fun consume(outcome: InstallOutcome) {
        latest.compareAndSet(outcome, null)
    }
}

/** Maps a final [PackageInstaller] status to the message shown to the user. */
internal fun installStatusText(status: Int, message: String?): UiText =
    when (status) {
        PackageInstaller.STATUS_SUCCESS -> uiText(R.string.status_installed)
        PackageInstaller.STATUS_FAILURE_ABORTED -> uiText(R.string.status_install_cancelled)
        PackageInstaller.STATUS_FAILURE_BLOCKED -> uiText(R.string.error_install_blocked)
        PackageInstaller.STATUS_FAILURE_CONFLICT -> uiText(R.string.error_install_conflict)
        PackageInstaller.STATUS_FAILURE_INCOMPATIBLE -> uiText(R.string.error_install_incompatible)
        PackageInstaller.STATUS_FAILURE_INVALID -> uiText(R.string.error_install_invalid)
        PackageInstaller.STATUS_FAILURE_STORAGE -> uiText(R.string.error_install_storage)
        else ->
            if (message.isNullOrBlank()) {
                uiText(R.string.error_install_failed)
            } else {
                uiText(R.string.error_install_failed_detail, message)
            }
    }
