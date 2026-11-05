package app.aqua.ui.canvas

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import app.aqua.engine.PAPER_BLACK
import app.aqua.engine.PAPER_WHITE

/** Papers to start from: the usual warm white through toned papers to black. */
private val PAPERS = listOf(
    PAPER_WHITE,
    0xFFFFFFFF.toInt(), // bright white
    0xFFF3E6C8.toInt(), // cream
    0xFFC9B08C.toInt(), // tan
    0xFFA88462.toInt(), // kraft
    0xFF9DA6A8.toInt(), // grey
    0xFF7C8FA6.toInt(), // blue-grey
    0xFF33363B.toInt(), // slate
    PAPER_BLACK,
)

/**
 * The paper's colour: the background under every layer. Watercolour is
 * transparent, so on dark paper it shows faintly; light ink shows best there.
 */
@Composable
fun PaperColorEditor(argb: Int, onChange: (Int) -> Unit) {
    // Kept locally so the sliders drag smoothly between engine round trips.
    var hsv by remember { mutableStateOf(toHsv(argb)) }
    fun set(h: FloatArray) {
        hsv = h
        onChange(android.graphics.Color.HSVToColor(h))
    }
    Column(Modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            for (c in PAPERS) {
                Swatch(Color(c), size = 26.dp, selected = c == argb) {
                    hsv = toHsv(c)
                    onChange(c)
                }
            }
        }
        Spacer(Modifier.size(8.dp))
        LabeledSlider("Hue", hsv[0] / 360f, { set(floatArrayOf(it * 360f, hsv[1], hsv[2])) }, format = { "${(it * 360).toInt()}°" })
        LabeledSlider("Tone", hsv[1], { set(floatArrayOf(hsv[0], it, hsv[2])) })
        LabeledSlider("Lightness", hsv[2], { set(floatArrayOf(hsv[0], hsv[1], it)) })
    }
}

private fun toHsv(argb: Int) = FloatArray(3).also { android.graphics.Color.colorToHSV(argb, it) }
