package top.madkarma.sniff

import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.lifecycle.viewModelScope
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.cancel
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.data.DownloadExporter
import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.DownloadSource
import top.madkarma.sniff.data.ReleaseChannel
import top.madkarma.sniff.ui.DownloadScreen
import top.madkarma.sniff.ui.DownloadViewModel
import java.io.File

/** Opt-in manual emulator harness: renders the PRODUCT screen with previously downloaded results. */
@RunWith(AndroidJUnit4::class)
class InteractiveUiTest {
    @Test
    fun inspectExportsOfRealMergedResult() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("interactiveUi") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val folder = File(File(context.filesDir, "last-test-result.txt").readText())
        val inventory = JSONObject(File(folder, "inventory.json").readText())
        val result = DownloadResult(
            File(folder, inventory.getString("output")),
            folder,
            inventory.getJSONArray("files").length(),
            inventory.getBoolean("merged"),
            inventory.getString("package")
        )
        assertTrue(result.apk.isFile)
        val fake = object : DownloadSource {
            override suspend fun download(
                packageId: String, channel: ReleaseChannel, resign: Boolean,
                onProgress: (String) -> Unit
            ) = result

        }
        val model = DownloadViewModel(fake, DownloadExporter(context.contentResolver))
        val ready = File(context.filesDir, "ui-test-ready")
        val finished = File(context.filesDir, "ui-test-finished")
        ready.delete()
        finished.delete()
        try {
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                scenario.onActivity { activity ->
                    activity.setContent { MaterialTheme { Surface { DownloadScreen(model) } } }
                    model.download(inventory.getString("package"), ReleaseChannel.STABLE)
                }
                val deadline = System.nanoTime() + 300_000_000_000L
                while (!finished.exists() && System.nanoTime() < deadline) {
                    if (model.state.value.result != null) ready.writeText(result.apk.name)
                    Thread.sleep(250)
                }
                assertTrue(
                    "Tester must create files/ui-test-finished within 5 minutes", finished.exists()
                )
            }
        } finally {
            model.viewModelScope.cancel()
            ready.delete()
            finished.delete()
        }
    }
}
