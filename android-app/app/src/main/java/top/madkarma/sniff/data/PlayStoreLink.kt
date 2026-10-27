package top.madkarma.sniff.data

import androidx.core.net.toUri

/**
 * Returns the package ID of a Google Play link, such as
 * https://play.google.com/store/apps/details?id=com.discord&hl=en, or the trimmed input otherwise.
 */
fun packageIdFromInput(input: String): String {
    val text = input.trim()
    val uri = text.toUri()

    val playLink = when (uri.scheme?.lowercase()) {
        "http", "https" -> uri.host.equals(
            "play.google.com", ignoreCase = true
        ) && uri.path?.trimEnd('/') == "/store/apps/details"

        "market" -> uri.host == "details"
        else -> false
    }

    return if (playLink) uri.getQueryParameter("id")?.trim() ?: text else text
}
