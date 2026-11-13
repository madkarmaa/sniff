package top.madkarma.sniff

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
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import top.madkarma.sniff.data.DownloadExporter
import top.madkarma.sniff.data.SniffRepository
import top.madkarma.sniff.ui.DownloadScreen
import top.madkarma.sniff.ui.DownloadViewModel

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        val factory = viewModelFactory {
            initializer {
                DownloadViewModel(
                    repository = SniffRepository(applicationContext),
                    exporter = DownloadExporter(contentResolver),
                )
            }
        }

        val model = ViewModelProvider(this, factory)[DownloadViewModel::class.java]

        setContent {
            val dark = isSystemInDarkTheme()
            val context = LocalContext.current
            // Dynamic color needs Android 12 (API 31); older versions use the static schemes.
            val colorScheme = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
            } else if (dark) {
                darkColorScheme()
            } else {
                lightColorScheme()
            }
            MaterialTheme(colorScheme = colorScheme) {
                Surface(Modifier.fillMaxSize()) { DownloadScreen(model) }
            }
        }
    }
}
