package top.madkarma.sniff.util

internal fun formatBytes(bytes: Long): String = "%.1f MB".format(bytes / 1_048_576.0)
