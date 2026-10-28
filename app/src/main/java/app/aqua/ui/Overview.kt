package app.aqua.ui

import android.hardware.display.DisplayManager
import android.view.Surface
import androidx.compose.foundation.Image
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/** Where the painting sits on screen, in window pixels: centre, size and rotation in degrees. */
data class CanvasFrame(val cx: Float, val cy: Float, val w: Float, val h: Float, val rotation: Float)

/** A painting coming back to the gallery from the editor: where it was on screen, and its fresh thumbnail. */
data class Arrival(val dir: String, val from: CanvasFrame, val thumb: ImageBitmap)

/** Corner radius of a gallery card; the flying painting rounds off to match. */
const val CARD_CORNER_DP = 6f

/**
 * Quarter turns (0..3) a painting is drawn at on screen: it stays the way up it
 * was when created ([home]) whichever way the screen is turned ([rotation]).
 */
fun screenTurn(home: Int, rotation: Int) = ((home - rotation) % 4 + 4) % 4

/** The screen's current rotation (a `Surface.ROTATION_*`), kept up to date, half turns included. */
@Composable
fun rememberDisplayRotation(): Int {
    val view = LocalView.current
    var rotation by remember { mutableIntStateOf(view.display?.rotation ?: Surface.ROTATION_0) }
    DisposableEffect(view) {
        val dm = view.context.getSystemService(DisplayManager::class.java)
        val listener = object : DisplayManager.DisplayListener {
            override fun onDisplayChanged(id: Int) {
                view.display?.rotation?.let { rotation = it }
            }

            override fun onDisplayAdded(id: Int) {}
            override fun onDisplayRemoved(id: Int) {}
        }
        dm.registerDisplayListener(listener, view.handler)
        onDispose { dm.unregisterDisplayListener(listener) }
    }
    return rotation
}

/** Lays the content out turned by [quarters] quarter turns, filling the box it is in. */
fun Modifier.turned(quarters: Int): Modifier = layout { m, c ->
    val odd = quarters % 2 == 1
    val p = m.measure(if (odd) Constraints.fixed(c.maxHeight, c.maxWidth) else Constraints.fixed(c.maxWidth, c.maxHeight))
    layout(c.maxWidth, c.maxHeight) {
        p.placeWithLayer((c.maxWidth - p.width) / 2, (c.maxHeight - p.height) / 2) { rotationZ = 90f * quarters }
    }
}

/**
 * The painting's thumbnail on its way from the editor into its gallery card,
 * at progress [t] (0 = where the canvas was, 1 = the card at [to], whose
 * painting is turned by [toTurn] quarter turns).
 */
@Composable
fun FlyingPainting(image: ImageBitmap, from: CanvasFrame, to: Rect, toTurn: Int, t: Float) {
    fun lerp(a: Float, b: Float) = a + (b - a) * t
    // Unwind to the card's own turn, the shorter way round.
    val end = 90f * toTurn
    val rot = end + (((from.rotation - end) % 360f + 540f) % 360f - 180f) * (1f - t)
    val odd = toTurn % 2 == 1
    val w = lerp(from.w, if (odd) to.height else to.width)
    val h = lerp(from.h, if (odd) to.width else to.height)
    val cx = lerp(from.cx, to.center.x)
    val cy = lerp(from.cy, to.center.y)
    Image(
        image,
        contentDescription = null,
        contentScale = ContentScale.FillBounds,
        modifier = Modifier
            // Placed by hand: it may start larger than the screen.
            .layout { m, _ ->
                val p = m.measure(Constraints.fixed(w.roundToInt().coerceAtLeast(1), h.roundToInt().coerceAtLeast(1)))
                layout(0, 0) { p.place((cx - w / 2f).roundToInt(), (cy - h / 2f).roundToInt()) }
            }
            .graphicsLayer { rotationZ = rot }
            .clip(RoundedCornerShape((CARD_CORNER_DP * t).dp)),
    )
}
