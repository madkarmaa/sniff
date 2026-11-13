package top.madkarma.sniff.data

import androidx.core.net.toUri

private val LINK_PATTERN = Regex("(?:https?|market)://\\S+", RegexOption.IGNORE_CASE)
private val LINK_TRAILING_PUNCTUATION = charArrayOf('.', ',', ';', ')', ']', '>', '"', '\'')

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

/**
 * Prepares text shared from another app, such as "Foo on Google Play: https://play.google.com/…",
 * for the package field: the package ID of its first link, or the trimmed text without a link.
 */
fun packageInputFromSharedText(text: String): String {
    val link = LINK_PATTERN.find(text)?.value?.trimEnd(*LINK_TRAILING_PUNCTUATION)
    return packageIdFromInput(link ?: text)
}
