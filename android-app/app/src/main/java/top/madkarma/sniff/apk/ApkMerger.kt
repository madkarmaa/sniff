package top.madkarma.sniff.apk

import com.reandroid.apk.ApkBundle
import com.reandroid.apk.ApkModule
import com.reandroid.archive.writer.ZipAligner
import com.reandroid.arsc.chunk.xml.AndroidManifestBlock
import java.io.File

private const val NATIVE_LIBRARY_ALIGNMENT = 16 * 1024
private val SIGNATURE_EXTENSIONS = listOf(".SF", ".RSA", ".DSA", ".EC")

/** Resource, DEX, and manifest merging follows APKEditor's m command. */
internal object ApkMerger {
    fun merge(
        input: File, output: File, packageId: String, versionCode: Long, resign: Boolean = false
    ) {
        BaseApkBundle().use { bundle ->
            bundle.loadApkDirectory(input, false)
            validateModules(bundle, packageId, versionCode)

            bundle.mergeModules(true).use { merged ->
                removeStoreMetadata(merged)
                if (resign) removeOriginalSignatures(merged)

                merged.setExtractNativeLibs(true)
                merged.refreshTable()
                merged.refreshManifest()

                writeOutput(merged, output, resign)
            }
        }
    }

    private fun validateModules(bundle: ApkBundle, packageId: String, versionCode: Long) {
        check(bundle.baseModule != null) { "The download has no base APK" }

        bundle.apkModuleList.forEach { module ->
            check(module.packageName == packageId) { "An APK has a different package ID" }

            val actualVersion = module.androidManifest.versionCode?.toLong()?.and(0xffffffffL)
            check(actualVersion == versionCode) { "An APK has a different version code" }
        }
    }

    private fun removeStoreMetadata(module: ApkModule) {
        // ARSCLib sanitizes split references; APKEditor removes this additional store metadata.
        module.androidManifest.listApplicationElementsByTag("meta-data").toList().forEach { node ->
            val name =
                node.searchAttributeByResourceId(AndroidManifestBlock.ID_name)?.valueAsString.orEmpty()

            val value =
                node.searchAttributeByResourceId(AndroidManifestBlock.ID_value)?.valueAsString

            val storeMetadata =
                name.startsWith("com.android.vending.") || name.startsWith("com.android.stamp.")

            val baseOnlyFusedModules =
                name == "com.android.dynamic.apk.fused.modules" && value == "base"

            if (storeMetadata || baseOnlyFusedModules) node.removeSelf()
        }
    }

    private fun removeOriginalSignatures(module: ApkModule) {
        module.apkSignatureBlock = null
        module.zipEntryMap.toArray().forEach { entry ->
            val path = entry.alias.uppercase()
            val signatureFile =
                path == "META-INF/MANIFEST.MF" || SIGNATURE_EXTENSIONS.any(path::endsWith)
            if (path.startsWith("META-INF/") && signatureFile) {
                module.removeInputSource(entry.alias)
            }
        }
    }

    private fun writeOutput(module: ApkModule, output: File, resign: Boolean) {
        if (!resign) {
            // APKEditor retains signature data, which does not authenticate the modified archive.
            writeAlignedApk(module, output)
            return
        }

        val unsigned = File(output.parentFile, "merged-unsigned.apk")
        try {
            writeAlignedApk(module, unsigned)
            LocalApkSigner.sign(unsigned, output)
        } finally {
            unsigned.delete()
        }
    }

    private fun writeAlignedApk(module: ApkModule, output: File) {
        val writer = module.createApkFileWriter(output)
        writer.zipAligner.setFileAlignment(
            ZipAligner.PREDICATE_NATIVE_LIBS, NATIVE_LIBRARY_ALIGNMENT
        )
        writer.write()
    }

    private class BaseApkBundle : ApkBundle() {
        // The API names the base explicitly; launcher heuristics miss service-only applications.
        override fun getBaseModule(): ApkModule? = getApkModule("base")
    }
}
