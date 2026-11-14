package top.madkarma.sniff

import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import top.madkarma.sniff.apk.SplitInstaller
import top.madkarma.sniff.data.DownloadExporter
import top.madkarma.sniff.data.ServerSettings
import top.madkarma.sniff.data.SniffRepository
import top.madkarma.sniff.data.packageInputFromSharedText
import top.madkarma.sniff.ui.DownloadScreen
import top.madkarma.sniff.ui.DownloadViewModel
import top.madkarma.sniff.ui.IncomingInput

class MainActivity : ComponentActivity() {
    // Held by the activity, not the ViewModel: a recreated activity starts empty, so a rotation
    // never applies the same shared text twice. The field itself survives through its saved state.
    private var incoming by mutableStateOf<IncomingInput?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        // A restored activity already holds the earlier prefill; so does one reopened from recents.
        val fromHistory = (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0
        if (savedInstanceState == null && !fromHistory) receive(intent)
        addOnNewIntentListener { receive(it) }

        val serverSettings = ServerSettings(applicationContext)
        val factory = viewModelFactory {
            initializer {
                DownloadViewModel(
                    repository =
                        SniffRepository(
                            applicationContext,
                            serverUrl = { serverSettings.serverUrl },
                        ),
                    exporter = DownloadExporter(contentResolver),
                    installer = SplitInstaller(applicationContext),
                )
            }
        }

        val model = ViewModelProvider(this, factory)[DownloadViewModel::class.java]

        setContent {
            val dark = isSystemInDarkTheme()
            val context = LocalContext.current
            // Dynamic color needs Android 12 (API 31); older versions use the static schemes.
            val colorScheme =
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
                } else if (dark) {
                    darkColorScheme()
                } else {
                    lightColorScheme()
                }
            MaterialTheme(colorScheme = colorScheme) {
                Surface(Modifier.fillMaxSize()) { DownloadScreen(model, serverSettings, incoming) }
            }
        }
    }

    /** Prefills the package field from an app shared to Sniff or from a Google Play link. */
    private fun receive(received: Intent) {
        val text =
            when (received.action) {
                Intent.ACTION_SEND -> received.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
                Intent.ACTION_VIEW -> received.dataString
                else -> null
            } ?: return

        val input = packageInputFromSharedText(text)
        if (input.isNotEmpty()) incoming = IncomingInput(input)
    }
}
