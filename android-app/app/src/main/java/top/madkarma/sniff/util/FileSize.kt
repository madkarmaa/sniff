package top.madkarma.sniff.util

import java.util.Locale

private const val BYTES_PER_UNIT = 1024.0
private val LARGE_UNITS = listOf("KB", "MB", "GB")

/** Formats [bytes] with binary units: "512 B", "3.4 KB", "12.0 MB", "1.5 GB". */
internal fun formatBytes(bytes: Long, locale: Locale = Locale.getDefault()): String {
    if (bytes < BYTES_PER_UNIT) return "$bytes B"

    var value = bytes / BYTES_PER_UNIT
    var unit = 0
    while (value >= BYTES_PER_UNIT && unit < LARGE_UNITS.lastIndex) {
        value /= BYTES_PER_UNIT
        unit++
    }

    return String.format(locale, "%.1f %s", value, LARGE_UNITS[unit])
}
