package top.madkarma.sniff.data

import org.json.JSONObject

private val UNSAFE_FILENAME_CHARACTERS = Regex("[^A-Za-z0-9._-]")
private const val MAX_FILENAME_LENGTH = 160

internal fun parseDownloadPlan(packageId: String, data: JSONObject): DownloadPlan {
    val item = data.getJSONObject("item")

    val details = item.getJSONObject("details").getJSONObject("app_details")
    check(details.getString("package_name") == packageId) { "API returned a different package" }

    val versionCode = details.getLong("version_code")

    val files =
        mutableListOf(
            RemoteFile(
                label = "base.apk",
                url = requiredUrl(data, "main_apk_url"),
                relativePath = "apks/base.apk",
                size = data.optSize("main_apk_size"),
                sha1 = data.optText("main_apk_sha1"),
                sha256 = data.optText("main_apk_sha256"),
            ),
        )

    val splitCount = addSplits(data, files)
    addAdditionalFiles(data, files)
    val assetPacks = addAssetPacks(data, files)

    data.optText("dex_metadata_url")?.let { files += RemoteFile("base.dm", it, "metadata/base.dm") }

    return DownloadPlan(
        packageId = packageId,
        versionCode = versionCode,
        title = item.optText("title") ?: packageId,
        files = files.toList(),
        splitCount = splitCount,
        failedAbis = failedAbis(data),
        assetPacks = assetPacks,
    )
}

/** The ABIs the server could not fetch, from the optional per-ABI "targets" of API v3. */
private fun failedAbis(data: JSONObject): List<String> {
    val targets = data.optJSONArray("targets") ?: return emptyList()

    return (0 until targets.length())
        .mapNotNull { index ->
            val target = targets.optJSONObject(index) ?: return@mapNotNull null
            // A target that does not say it failed is not reported as failed.
            if (target.optBoolean("success", true)) null else target.optText("abi")
        }
        .distinct()
}

private fun addSplits(data: JSONObject, files: MutableList<RemoteFile>): Int {
    val splits = data.getJSONArray("splits")

    for (index in 0 until splits.length()) {
        val split = splits.getJSONObject(index)
        val name = safeFileName(split.optText("name") ?: "split-$index")
        val numberedName = index.toString().padStart(4, '0')

        files +=
            RemoteFile(
                label = "$name.apk",
                url = requiredUrl(split, "download_url"),
                relativePath = "apks/split-$numberedName-$name.apk",
                size = split.optSize("size"),
                sha1 = split.optText("sha1"),
                sha256 = split.optText("sha256"),
            )
    }

    return splits.length()
}

private fun addAdditionalFiles(data: JSONObject, files: MutableList<RemoteFile>) {
    val additional = data.getJSONArray("additional_files")
    val names = mutableSetOf<String>()

    for (index in 0 until additional.length()) {
        val file = additional.getJSONObject(index)

        val name = safeFileName(file.optText("filename") ?: "additional-$index")
        check(names.add(name)) { "API returned duplicate additional filenames" }

        files +=
            RemoteFile(
                label = name,
                url = requiredUrl(file, "download_url"),
                relativePath = "additional/$name",
                size = file.optSize("size"),
                sha1 = file.optText("sha1"),
                sha256 = file.optText("sha256"),
            )
    }
}

/**
 * Adds the files of the asset packs, which servers before API 3.1 do not send, and returns the
 * packs. They go to their own folder, since they cannot be installed or merged like splits.
 */
private fun addAssetPacks(data: JSONObject, files: MutableList<RemoteFile>): List<AssetPack> {
    val packFiles = data.optJSONArray("asset_packs") ?: return emptyList()
    val names = mutableSetOf<String>()
    val packs = mutableListOf<AssetPack>()

    for (index in 0 until packFiles.length()) {
        val file = packFiles.getJSONObject(index)
        val packName = checkNotNull(file.optText("name")) { "API omitted an asset pack name" }
        val delivery =
            AssetPackDelivery.fromApiName(file.optText("delivery_mode").orEmpty())
                ?: AssetPackDelivery.ON_DEMAND

        val name = safeFileName(file.optText("slice_id") ?: packName)
        check(names.add(name)) { "API returned duplicate asset pack files" }

        files +=
            RemoteFile(
                label = "$name.apk",
                url = requiredUrl(file, "download_url"),
                relativePath = "$ASSET_PACK_FOLDER/$name.apk",
                size = file.optSize("size"),
                sha256 = file.optText("sha256"),
            )
        packs += AssetPack(packName, delivery)
    }

    return packs.distinct()
}

private fun requiredUrl(json: JSONObject, key: String): String =
    checkNotNull(json.optText(key)) { "API omitted a required download URL ($key)" }

private fun safeFileName(value: String): String =
    value.replace(UNSAFE_FILENAME_CHARACTERS, "_").take(MAX_FILENAME_LENGTH).takeIf {
        it.isNotBlank() && it != "." && it != ".."
    } ?: "file"

/** Returns the value of [key], or null when it is missing, JSON null, or blank. */
internal fun JSONObject.optText(key: String): String? =
    if (isNull(key)) null else optString(key).takeIf { it.isNotBlank() }

/**
 * Returns the byte count at [key], or null when it is missing, JSON null, or not a positive number.
 */
private fun JSONObject.optSize(key: String): Long? =
    if (isNull(key)) null else optLong(key, -1L).takeIf { it > 0 }
