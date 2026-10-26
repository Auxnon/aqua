package app.aqua

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import app.aqua.engine.Native
import app.aqua.ui.canvas.CanvasScreen
import app.aqua.ui.gallery.GalleryScreen
import app.aqua.ui.theme.AquaTheme
import java.io.File

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val app = application as AquaApp
        setContent {
            AquaTheme {
                // The painting directory that is open, if any; survives rotation.
                var open by rememberSaveable { mutableStateOf<String?>(null) }
                val dir = open?.let(::File)
                if (dir == null) {
                    GalleryScreen(onOpen = { open = it.path })
                } else {
                    // After process death the engine is empty again; reload what was open.
                    remember(dir) { if (Native.query(app.engine) == "{}") Native.load(app.engine, dir.path) }
                    CanvasScreen(dir, onExit = { open = null })
                }
            }
        }
    }
}
