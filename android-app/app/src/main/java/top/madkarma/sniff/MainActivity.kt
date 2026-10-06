package top.madkarma.sniff

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.Modifier
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
            val colorScheme = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()
            MaterialTheme(colorScheme = colorScheme) {
                Surface(Modifier.fillMaxSize()) { DownloadScreen(model) }
            }
        }
    }
}
