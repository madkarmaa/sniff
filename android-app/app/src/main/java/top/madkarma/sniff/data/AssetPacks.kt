package top.madkarma.sniff.data

import com.reandroid.apk.ApkModule
import com.reandroid.arsc.chunk.xml.ResXmlElement
import java.io.File
import top.madkarma.sniff.R

/** The folder of a download that holds the asset packs that cannot be installed as splits. */
internal const val ASSET_PACK_FOLDER = "asset_packs"

/** The limit of the server, which sends every name to Google Play. */
private const val MAX_ASSET_PACK_NAMES = 20
private val ASSET_PACK_NAME = Regex("[A-Za-z0-9_]{1,100}")
private val NAME_SEPARATORS = Regex("[,\\s]+")

/** When Google Play delivers an asset pack; [apiName] is the name of the API and the cache. */
enum class AssetPackDelivery(val apiName: String) {
    INSTALL_TIME("install_time"),
    FAST_FOLLOW("fast_follow"),
    ON_DEMAND("on_demand");

    companion object {
        fun fromApiName(name: String): AssetPackDelivery? = entries.find { it.apiName == name }
    }
}

data class AssetPack(val name: String, val delivery: AssetPackDelivery)

/**
 * The on-demand asset pack names typed by the user, separated by commas or spaces. Google Play does
 * not list these packs, so only the user can name them.
 */
internal fun parseAssetPackNames(input: String): List<String> {
    val names = input.split(NAME_SEPARATORS).filter { it.isNotEmpty() }.distinct()
    if (names.size > MAX_ASSET_PACK_NAMES || !names.all(ASSET_PACK_NAME::matches)) {
        throw AppException(uiText(R.string.error_invalid_asset_packs, MAX_ASSET_PACK_NAMES))
    }
    return names
}

/**
 * The install-time asset packs among the split APKs in [apkFolder]. Google Play delivers them like
 * any other split, so only their manifest tells them apart from feature modules.
 */
internal fun installTimeAssetPacks(apkFolder: File): List<AssetPack> =
    apkFolder
        .listFiles { file -> file.extension == "apk" && file.name != "base.apk" }
        .orEmpty()
        .sortedBy { it.name }
        .mapNotNull { file ->
            ApkModule.loadApkFile(file).use { apk ->
                val manifest = apk.androidManifest
                val split = manifest.split
                if (split != null && isAssetPack(manifest.manifestElement)) {
                    AssetPack(split, AssetPackDelivery.INSTALL_TIME)
                } else {
                    null
                }
            }
        }

/** Whether the manifest declares `<dist:module dist:type="asset-pack">`. */
private fun isAssetPack(manifest: ResXmlElement): Boolean =
    manifest.getElements().asSequence().filterIsInstance<ResXmlElement>().any { element ->
        element.name == "module" &&
            element.searchAttributeByName("type")?.valueAsString == "asset-pack"
    }
