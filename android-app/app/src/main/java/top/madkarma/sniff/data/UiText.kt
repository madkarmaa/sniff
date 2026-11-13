package top.madkarma.sniff.data

import androidx.annotation.StringRes
import top.madkarma.sniff.R
import java.io.IOException
import java.io.InterruptedIOException
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import javax.net.ssl.SSLException

/** A user-visible message; the UI resolves it, so the data layer needs no Context. */
sealed interface UiText {
    data class Resource(@param:StringRes val id: Int, val args: List<Any> = emptyList()) : UiText

    /** Text that is already final, such as an error message written by the server. */
    data class Raw(val value: String) : UiText
}

internal fun uiText(@StringRes id: Int, vararg args: Any): UiText =
    UiText.Resource(id, args.toList())

/** An error the app raises itself, carrying the message shown to the user. */
internal class AppException(val text: UiText) : Exception(text.toString())

internal fun Throwable.toDownloadError(): UiText = when (this) {
    is UnknownHostException, is ConnectException -> uiText(R.string.error_offline)
    // OkHttp reports its call timeout as a plain InterruptedIOException.
    is SocketTimeoutException, is InterruptedIOException -> uiText(R.string.error_timeout)
    is SSLException -> uiText(R.string.error_secure_connection)
    is IOException -> uiText(R.string.error_network, message ?: javaClass.simpleName)
    else -> toUiText(R.string.error_download_failed)
}

internal fun Throwable.toSaveError(): UiText = toUiText(R.string.error_save_failed)

internal fun Throwable.toInstallError(): UiText = toUiText(R.string.error_install_failed)

private fun Throwable.toUiText(@StringRes fallback: Int): UiText = when (this) {
    is AppException -> text
    else -> message?.let { UiText.Raw(it) } ?: uiText(fallback)
}
