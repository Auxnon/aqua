package app.aqua.ui.canvas

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.aqua.ui.theme.AquaColors
import kotlinx.coroutines.launch

/**
 * The water cup, seen from above. Touching it dips the brush, filling it with
 * water again; the ring round the rim shows how much the brush still holds.
 * Given an [ink] colour it is an ink well instead, for the pen.
 */
@Composable
fun WaterCup(charge: Float, onDip: () -> Unit, modifier: Modifier = Modifier, ink: Color? = null, size: Dp = 60.dp) {
    val scope = rememberCoroutineScope()
    val ripple = remember { Animatable(1f) }
    val shown by animateFloatAsState(charge.coerceIn(0f, 1f), tween(250), label = "charge")
    fun dip() {
        onDip()
        scope.launch {
            ripple.snapTo(0f)
            ripple.animateTo(1f, tween(700, easing = LinearEasing))
        }
    }
    Canvas(
        modifier
            .size(size)
            .semantics {
                contentDescription = if (ink != null) "Ink well: dip the pen" else "Water cup: dip the brush"
                role = Role.Button
                onClick { dip(); true }
            }
            // On touch down, as a brush is dipped, not on release.
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown().consume()
                    dip()
                }
            },
    ) {
        val c = center
        val r = this.size.minDimension / 2
        // The cup's shadow and rim.
        drawCircle(Color.Black.copy(alpha = 0.18f), r, c + Offset(0f, r * 0.05f))
        drawCircle(AquaColors.Panel, r)
        if (ink == null) {
            drawCircle(Color.White.copy(alpha = 0.85f), r * 0.86f)
            // Water: clear, a little blue, deeper in the middle.
            drawCircle(
                Brush.radialGradient(
                    listOf(Color(0xFF9CC9E8).copy(alpha = 0.75f), Color(0xFFCFE6F5).copy(alpha = 0.6f)),
                    center = c - Offset(r * 0.2f, r * 0.2f),
                    radius = r,
                ),
                r * 0.78f,
            )
            // A highlight on the water.
            drawCircle(Color.White.copy(alpha = 0.55f), r * 0.16f, c - Offset(r * 0.3f, r * 0.32f))
        } else {
            // A squat glass well: a thick, smoky rim round a narrow opening of ink.
            drawCircle(
                Brush.radialGradient(
                    listOf(Color(0xFF5A6068), Color(0xFF2A2D33)),
                    center = c - Offset(r * 0.3f, r * 0.3f),
                    radius = r * 1.2f,
                ),
                r * 0.86f,
            )
            drawCircle(Color.White.copy(alpha = 0.18f), r * 0.86f, style = Stroke(width = r * 0.04f))
            drawCircle(Color.Black.copy(alpha = 0.6f), r * 0.6f)
            drawCircle(
                Brush.radialGradient(
                    listOf(lerp(ink, Color.White, 0.18f), lerp(ink, Color.Black, 0.45f)),
                    center = c - Offset(r * 0.15f, r * 0.15f),
                    radius = r * 0.6f,
                ),
                r * 0.52f,
            )
            // Light on the glass, and a glint on the ink.
            drawArc(Color.White.copy(alpha = 0.35f), 200f, 60f, false, c - Offset(r * 0.73f, r * 0.73f), Size(r * 1.46f, r * 1.46f), style = Stroke(r * 0.08f, cap = StrokeCap.Round))
            drawCircle(Color.White.copy(alpha = 0.45f), r * 0.09f, c - Offset(r * 0.2f, r * 0.22f))
        }
        // Ripples spreading out from a dip.
        val t = ripple.value
        if (t < 1f) {
            for (k in 0..1) {
                val tk = (t * 1.3f - k * 0.3f).coerceIn(0f, 1f)
                if (tk > 0f && tk < 1f) {
                    drawCircle(
                        Color.White.copy(alpha = (if (ink == null) 0.8f else 0.45f) * (1f - tk)),
                        r * (if (ink == null) 0.78f else 0.52f) * tk,
                        style = Stroke(width = r * 0.07f),
                    )
                }
            }
        }
        // What the brush holds, round the rim, from the top.
        val w = r * 0.12f
        val inset = w / 2 + r * 0.01f
        val arc = Size(2 * (r - inset), 2 * (r - inset))
        val tl = Offset(c.x - r + inset, c.y - r + inset)
        drawArc(AquaColors.TextDim.copy(alpha = 0.25f), 0f, 360f, false, tl, arc, style = Stroke(w))
        if (shown > 0.005f) {
            val col = if (shown < 0.15f) Color(0xFFD08040) else AquaColors.Accent
            drawArc(col, -90f, 360f * shown, false, tl, arc, style = Stroke(w, cap = StrokeCap.Round))
        }
    }
}

/** Brushes that run dry and have to be dipped: a cup of water. */
val CupIcon: ImageVector = ImageVector.Builder("Cup", 24.dp, 24.dp, 24f, 24f).apply {
    path(stroke = SolidColor(Color.Black), strokeLineWidth = 1.8f, strokeLineJoin = StrokeJoin.Round, strokeLineCap = StrokeCap.Round) {
        // The cup.
        moveTo(4f, 7f)
        lineTo(16f, 7f)
        lineTo(16f, 15f)
        arcToRelative(4f, 4f, 0f, false, true, -4f, 4f)
        lineTo(8f, 19f)
        arcToRelative(4f, 4f, 0f, false, true, -4f, -4f)
        close()
        // Handle.
        moveTo(16f, 9.5f)
        lineTo(17.5f, 9.5f)
        arcToRelative(2.5f, 2.5f, 0f, false, true, 0f, 5f)
        lineTo(16f, 14.5f)
        // Water line.
        moveTo(6.5f, 11f)
        curveTo(7.5f, 10.2f, 8.5f, 10.2f, 9.5f, 11f)
        curveTo(10.5f, 11.8f, 11.5f, 11.8f, 12.5f, 11f)
    }
}.build()
