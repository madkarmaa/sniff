package top.madkarma.sniff.apk

import com.reandroid.apk.ApkModule
import java.io.File
import java.io.OutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import org.json.JSONArray
import org.json.JSONObject
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.util.copyWithCancellation

private const val XAPK_VERSION = 2
private const val BASE_SPLIT_ID = "base"
private val SPLIT_NUMBER_PREFIX = Regex("^split-\\d+-")

/** A single APK without expansion files gains nothing from a bundle. */
internal fun DownloadResult.canExportXapk(): Boolean = merged || hasExpansionFiles()

/**
 * Writes a download as an XAPK, the split bundle of APKPure that split APK installers such as SAI
 * and App Manager read. It holds the original APKs, so they keep the developer's signature.
 *
 * XAPK has no official specification. The manifest carries the fields that the known readers
 * require, and the layout follows the archives APKPure publishes.
 */
internal object XapkWriter {
    private class Entry(val file: File, val path: String, val splitId: String? = null)

    suspend fun write(result: DownloadResult, output: OutputStream) {
        val apkFolder = File(result.folder, "apks")
        val base = File(apkFolder, "base.apk")

        ApkModule.loadApkFile(base).use { apk ->
            val packageId = apk.packageName
            val manifest = apk.androidManifest

            val splits =
                apkFolder
                    .listFiles { file -> file.extension == "apk" && file != base }
                    .orEmpty()
                    .sortedBy { it.name }
                    .map { file ->
                        val splitId = file.nameWithoutExtension.replace(SPLIT_NUMBER_PREFIX, "")
                        Entry(file, "$splitId.apk", splitId)
                    }
            val apks = listOf(Entry(base, "$packageId.apk", BASE_SPLIT_ID)) + splits

            // Installers copy each expansion file to the path it has in the archive.
            val expansions =
                File(result.folder, "additional")
                    .listFiles { file -> file.extension == "obb" }
                    .orEmpty()
                    .sortedBy { it.name }
                    .map { Entry(it, "Android/obb/$packageId/${it.name}") }

            val minSdk = manifest.minSdkVersion ?: 1
            val xapkManifest =
                JSONObject()
                    .put("xapk_version", XAPK_VERSION)
                    .put("package_name", packageId)
                    .put("name", result.title)
                    // An unsigned 32-bit value, as in the API and the cache inventory.
                    .put("version_code", manifest.versionCode?.toLong()?.and(0xffffffffL))
                    .put("version_name", manifest.versionName.orEmpty())
                    .put("min_sdk_version", minSdk)
                    .put("target_sdk_version", manifest.targetSdkVersion ?: minSdk)
                    .put("total_size", (apks + expansions).sumOf { it.file.length() })
                    .put("split_configs", JSONArray(splits.map { it.splitId }))
                    .put(
                        "split_apks",
                        JSONArray(
                            apks.map { JSONObject().put("file", it.path).put("id", it.splitId) },
                        ),
                    )
                    .put(
                        "expansions",
                        JSONArray(
                            expansions.map {
                                JSONObject()
                                    .put("file", it.path)
                                    .put("install_location", "EXTERNAL_STORAGE")
                                    .put("install_path", it.path)
                            },
                        ),
                    )

            ZipOutputStream(output).use { zip ->
                zip.putNextEntry(ZipEntry("manifest.json"))
                zip.write(xapkManifest.toString(2).toByteArray())
                zip.closeEntry()

                (apks + expansions).forEach { entry ->
                    currentCoroutineContext().ensureActive()

                    zip.putNextEntry(ZipEntry(entry.path))
                    entry.file.inputStream().use { it.copyWithCancellation(zip) }
                    zip.closeEntry()
                }
            }
        }
    }
}
