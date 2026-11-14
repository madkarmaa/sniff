package top.madkarma.sniff.data

internal data class DownloadPlan(
    val packageId: String,
    val versionCode: Long,
    val title: String,
    val files: List<RemoteFile>,
    val splitCount: Int,
    /** ABIs the server could not fetch; the merged APK may lack native code for them. */
    val failedAbis: List<String> = emptyList(),
) {
    val hasSplits: Boolean
        get() = splitCount > 0
}

/** A file to download. [size] and the hashes are only known from API v3 on and are optional. */
internal data class RemoteFile(
    val label: String,
    val url: String,
    val relativePath: String,
    val size: Long? = null,
    val sha1: String? = null,
    val sha256: String? = null,
)
