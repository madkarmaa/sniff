package top.madkarma.sniff.data

import java.io.File
import org.json.JSONArray
import org.json.JSONObject

internal const val INVENTORY_FILE = "inventory.json"

/**
 * The inventory of a complete cached download; it is written last, so its presence marks
 * completion.
 */
internal data class CachedDownload(
    val versionCode: Long,
    val resigned: Boolean,
    val files: List<String>,
    val output: String,
    /** The ABIs this download lacks because the server could not fetch them. */
    val failedAbis: List<String>,
)

internal fun readCachedDownload(folder: File, plan: DownloadPlan): CachedDownload? {
    val inventory = File(folder, INVENTORY_FILE)
    if (!inventory.isFile) return null

    val cached =
        runCatching {
            val json = JSONObject(inventory.readText())
            val files = json.getJSONArray("files")
            // Inventories written before API v3 have no list; they were complete downloads.
            val failed = json.optJSONArray("failed_abis")
            CachedDownload(
                versionCode = json.getLong("version_code"),
                resigned = json.getBoolean("resigned"),
                files = List(files.length()) { files.getString(it) },
                output = json.getString("output"),
                failedAbis =
                    failed?.let { abis -> List(abis.length()) { abis.getString(it) } }.orEmpty(),
            )
        }
            .getOrNull() ?: return null

    val complete = cached.files.all { File(folder, it).isFile }
    // A download that lacks ABIs is only reused if the server still cannot offer them; otherwise
    // downloading again completes it.
    val sufficient = plan.failedAbis.containsAll(cached.failedAbis)
    return cached.takeIf { it.versionCode == plan.versionCode && complete && sufficient }
}

internal fun writeInventory(
    plan: DownloadPlan,
    channel: ReleaseChannel,
    folder: File,
    output: File,
    resign: Boolean,
    failedAbis: List<String>,
) {
    // Signed download URLs expire and must not be included in exported inventories.
    val inventory =
        JSONObject()
            .put("package", plan.packageId)
            .put("version_code", plan.versionCode)
            .put("channel", channel.apiName)
            .put("merged", plan.hasSplits)
            .put("resigned", plan.hasSplits && resign)
            .put("files", JSONArray(plan.files.map { it.relativePath }))
            .put("output", output.name)
            .put("failed_abis", JSONArray(failedAbis))

    File(folder, INVENTORY_FILE).writeText(inventory.toString(2))
}
