package app.aqua.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/**
 * Where the painting sits on screen, in window pixels: centre, size and rotation
 * in degrees. [turn] is the part of the rotation that came from the device
 * turning under the paper rather than from the painter.
 */
data class CanvasFrame(val cx: Float, val cy: Float, val w: Float, val h: Float, val rotation: Float, val turn: Float = 0f)

/** A painting coming back to the gallery from the editor: where it was on screen, and its fresh thumbnail. */
data class Arrival(val dir: String, val from: CanvasFrame, val thumb: ImageBitmap)

/** Corner radius of a gallery card; the flying painting rounds off to match. */
const val CARD_CORNER_DP = 6f

/**
 * The painting's thumbnail on its way from the editor into its gallery card,
 * at progress [t] (0 = where the canvas was, 1 = the card at [to]).
 */
@Composable
fun FlyingPainting(image: ImageBitmap, from: CanvasFrame, to: Rect, t: Float) {
    fun lerp(a: Float, b: Float) = a + (b - a) * t
    // Unwind the shorter way round. The painter's own rotation unwinds on the
    // way; a turn that came from the device stays as it was held until the
    // painting lands, then settles upright into the card.
    fun wrap(a: Float) = ((a % 360f) + 540f) % 360f - 180f
    val own = wrap(from.rotation - from.turn)
    val turn = wrap(from.turn)
    val settle = ((t - 0.55f) / 0.45f).coerceIn(0f, 1f).let { it * it * (3f - 2f * it) }
    val rot = own * (1f - t) + turn * (1f - settle)
    val w = lerp(from.w, to.width)
    val h = lerp(from.h, to.height)
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
