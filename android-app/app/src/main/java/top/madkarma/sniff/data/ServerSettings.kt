package top.madkarma.sniff.data

import android.content.Context
import androidx.core.content.edit
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

const val DEFAULT_SERVER_URL = "https://sniff.madkarma.top"

private const val PREFERENCES_NAME = "settings"
private const val SERVER_URL_KEY = "server_url"

/** The Sniff server the app asks: the official one unless the user chose their own deployment. */
class ServerSettings(context: Context) {
    private val preferences = context.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    var serverUrl: String
        get() =
            preferences.getString(SERVER_URL_KEY, null)?.let(::normalizeServerUrl)
                ?: DEFAULT_SERVER_URL
        set(value) = preferences.edit {
            // Not storing the default lets a new default reach users who never changed it.
            if (value == DEFAULT_SERVER_URL) remove(SERVER_URL_KEY)
            else putString(SERVER_URL_KEY, value)
        }
}

/**
 * Returns [input] as a server URL without a trailing slash, or null when it is not an HTTPS URL
 * without query or fragment. The manifest forbids cleartext traffic, so HTTP could never connect.
 */
fun normalizeServerUrl(input: String): String? {
    val url = input.trim().toHttpUrlOrNull() ?: return null
    if (!url.isHttps || url.query != null || url.fragment != null) return null
    return url.toString().trimEnd('/')
}
