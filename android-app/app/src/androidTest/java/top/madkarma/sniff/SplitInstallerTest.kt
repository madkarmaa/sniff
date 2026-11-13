package top.madkarma.sniff

import android.content.pm.PackageInstaller
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.apk.installStatusText
import top.madkarma.sniff.apk.selectInstallFiles
import top.madkarma.sniff.data.UiText

@RunWith(AndroidJUnit4::class)
class SplitInstallerTest {
    private fun split(index: Int, name: String) =
        "split-${index.toString().padStart(4, '0')}-$name.apk"

    private val splits = listOf(
        "base.apk",
        split(0, "config.arm64_v8a"),
        split(1, "config.armeabi_v7a"),
        split(2, "config.x86"),
        split(3, "config.x86_64"),
        split(4, "config.en"),
        split(5, "config.xxhdpi"),
        split(6, "feature_video"),
        split(7, "feature_video.config.arm64_v8a"),
        split(8, "feature_video.config.x86_64"),
    )

    private val shared = listOf(
        "base.apk", split(4, "config.en"), split(5, "config.xxhdpi"), split(6, "feature_video")
    )

    /** The selection keeps the input order; only the files matter here. */
    private fun assertSelected(expected: List<String>, files: List<String>, abis: List<String>) =
        assertEquals(expected.sorted(), selectInstallFiles(files, abis).sorted())

    @Test
    fun keepsOnlyTheFirstSupportedAbi() {
        assertSelected(
            shared + listOf(split(0, "config.arm64_v8a"), split(7, "feature_video.config.arm64_v8a")),
            splits,
            listOf("arm64-v8a", "armeabi-v7a", "armeabi"),
        )
        assertSelected(
            shared + listOf(split(3, "config.x86_64"), split(8, "feature_video.config.x86_64")),
            splits,
            listOf("x86_64", "x86"),
        )
        assertSelected(shared + split(2, "config.x86"), splits, listOf("x86"))
    }

    @Test
    fun fallsBackToTheNextAbiWithASplit() {
        val names = listOf("base.apk", split(0, "config.armeabi_v7a"), split(1, "config.x86"))

        assertSelected(
            listOf("base.apk", split(0, "config.armeabi_v7a")),
            names,
            listOf("arm64-v8a", "armeabi-v7a", "armeabi"),
        )
    }

    @Test
    fun dropsAllAbiSplitsWhenNoneMatchesTheDevice() {
        assertSelected(shared, splits, listOf("riscv64"))
    }

    @Test
    fun keepsEverythingWithoutAbiSplits() {
        assertSelected(listOf("base.apk"), listOf("base.apk"), listOf("arm64-v8a"))
        assertSelected(shared, shared, listOf("arm64-v8a"))
    }

    @Test
    fun mapsFailureStatusesToTheirOwnMessages() {
        val statuses = listOf(
            PackageInstaller.STATUS_FAILURE_ABORTED,
            PackageInstaller.STATUS_FAILURE_BLOCKED,
            PackageInstaller.STATUS_FAILURE_CONFLICT,
            PackageInstaller.STATUS_FAILURE_INCOMPATIBLE,
            PackageInstaller.STATUS_FAILURE_INVALID,
            PackageInstaller.STATUS_FAILURE_STORAGE,
            PackageInstaller.STATUS_FAILURE,
        )
        val messages = statuses.map { installStatusText(it, null) }
        assertEquals(statuses.size, messages.toSet().size)

        // Only the generic failure shows the message of the system.
        assertEquals(
            installStatusText(PackageInstaller.STATUS_FAILURE_CONFLICT, null),
            installStatusText(PackageInstaller.STATUS_FAILURE_CONFLICT, "INSTALL_FAILED"),
        )
        assertNotEquals(
            installStatusText(PackageInstaller.STATUS_FAILURE, null),
            installStatusText(PackageInstaller.STATUS_FAILURE, "INSTALL_FAILED"),
        )
        val detail = installStatusText(PackageInstaller.STATUS_FAILURE, "INSTALL_FAILED")
        assertEquals(listOf<Any>("INSTALL_FAILED"), (detail as UiText.Resource).args)
    }
}
