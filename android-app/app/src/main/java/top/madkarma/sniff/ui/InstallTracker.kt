package top.madkarma.sniff.ui

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import top.madkarma.sniff.R
import top.madkarma.sniff.apk.InstallEvents
import top.madkarma.sniff.apk.InstallOutcome
import top.madkarma.sniff.apk.SplitInstaller
import top.madkarma.sniff.apk.hasExpansionFiles
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.toInstallError
import top.madkarma.sniff.data.uiText

/**
 * Installs downloads for [DownloadViewModel] and recognizes the outcome of the session it waits
 * for. The system reports that outcome later, once the user answered its dialog.
 */
internal class InstallTracker(
    private val installer: SplitInstaller,
    private val state: MutableStateFlow<DownloadUiState>,
) {
    // Set from the IO thread before the commit, so an outcome that arrives right away is still
    // recognized.
    @Volatile private var awaitedSession = NO_SESSION
    private var hasExpansionFiles = false

    /** True from the commit until the outcome arrives or the user stops waiting. */
    val isAwaiting: Boolean
        get() = awaitedSession != NO_SESSION

    /** Installs the original APK files of [result]; the state stays busy until [onOutcome]. */
    suspend fun install(result: DownloadResult) {
        hasExpansionFiles = result.hasExpansionFiles()
        try {
            val session = installer.install(result) { awaitedSession = it }
            // The outcome may have been handled already while this coroutine was resuming.
            if (awaitedSession == session) {
                state.update { it.copy(message = uiText(R.string.status_install_confirm)) }
            }
        } catch (_: CancellationException) {
            awaitedSession = NO_SESSION
            state.update {
                it.copy(busy = false, message = uiText(R.string.status_install_cancelled))
            }
        } catch (error: Exception) {
            awaitedSession = NO_SESSION
            state.update { it.copy(busy = false, error = true, message = error.toInstallError()) }
        }
    }

    /** Stops waiting for the system dialog; an outcome that still arrives is shown anyway. */
    fun stopAwaiting() {
        awaitedSession = NO_SESSION
        state.update {
            it.copy(busy = false, error = false, message = uiText(R.string.status_install_detached))
        }
    }

    /** Shows [outcome], also one of a session this instance did not start. */
    fun onOutcome(outcome: InstallOutcome) {
        InstallEvents.consume(outcome)

        val awaited = isAwaiting && outcome.sessionId == awaitedSession
        // A late outcome must not replace the progress of something else that is running.
        if (!awaited && state.value.busy) return

        if (awaited) awaitedSession = NO_SESSION
        val text =
            if (awaited && outcome.success && hasExpansionFiles) {
                uiText(R.string.status_installed_obb)
            } else {
                outcome.text
            }
        state.update { it.copy(busy = false, error = outcome.error, message = text) }
    }

    private companion object {
        const val NO_SESSION = -1
    }
}
