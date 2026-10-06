package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.android.apksig.ApkVerifier
import com.reandroid.apk.ApkModule
import com.reandroid.archive.block.SignatureId
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.apk.ApkMerger
import java.io.File

@RunWith(AndroidJUnit4::class)
class ApkMergerTest {
    /** Supply mergeInputs pointing to a staged directory of genuine base/split APKs. */
    @Test
    fun resignIsOptInAndReusesTheDeviceKey() {
        val inputPath = InstrumentationRegistry.getArguments().getString("mergeInputs")
        assumeTrue("Supply staged APK inputs to check both merge modes", inputPath != null)
        val input = File(inputPath!!)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val folder = File.createTempFile("merge-test-", "", context.cacheDir).apply {
            delete()
            check(mkdir())
        }

        try {
            ApkModule.loadApkFile(File(input, "base.apk")).use { base ->
                val version = checkNotNull(base.androidManifest.versionCode).toLong().and(0xffffffffL)
                val preserved = File(folder, "preserved.apk")
                ApkMerger.merge(input, preserved, base.packageName, version)
                ApkModule.loadApkFile(preserved).use { merged ->
                    val source = checkNotNull(base.apkSignatureBlock)
                    val result = checkNotNull(merged.apkSignatureBlock)
                    source.filter { it.id != SignatureId.PADDING }.forEach { signature ->
                        assertArrayEquals(
                            signature.bytes,
                            checkNotNull(result.getSignature(signature.id)).bytes,
                        )
                    }
                }

                val certificates = (1..2).map { attempt ->
                    val output = File(folder, "resigned-$attempt.apk")
                    ApkMerger.merge(input, output, base.packageName, version, resign = true)
                    val verification = ApkVerifier.Builder(output)
                        .setMinCheckedPlatformVersion(24).build().verify()
                    assertTrue(verification.errors.toString(), verification.isVerified)
                    assertFalse(verification.isVerifiedUsingV1Scheme)
                    assertTrue(verification.isVerifiedUsingV2Scheme)
                    assertTrue(verification.isVerifiedUsingV3Scheme)
                    assertFalse(verification.isVerifiedUsingV4Scheme)
                    verification.signerCertificates.single().encoded
                }
                assertArrayEquals(certificates[0], certificates[1])
                assertFalse(File(folder, "merged-unsigned.apk").exists())
            }
        } finally {
            folder.deleteRecursively()
        }
    }
}
