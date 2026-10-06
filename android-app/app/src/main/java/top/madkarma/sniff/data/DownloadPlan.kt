package top.madkarma.sniff.data

internal data class DownloadPlan(
    val packageId: String,
    val versionCode: Long,
    val title: String,
    val files: List<RemoteFile>,
    val splitCount: Int,
) {
    val hasSplits: Boolean get() = splitCount > 0
}

internal data class RemoteFile(
    val label: String,
    val url: String,
    val relativePath: String,
)
