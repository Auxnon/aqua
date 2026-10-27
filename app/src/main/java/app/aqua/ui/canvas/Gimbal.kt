package app.aqua.ui.canvas

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.view.Surface
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aqua.ui.theme.AquaColors
import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin
import kotlin.math.sqrt

/** How far the phone has to tip, from where it was held, for the steepest paper. */
private const val STEEPEST_DEG = 50.0

/** Tilts shorter than this count as level, so a hand's tremor doesn't stir the paint. */
private const val DEAD_ZONE = 0.04f

/** Steepness 0..1 as degrees, for the readout. */
private fun degrees(m: Float) = Math.toDegrees(asin(m * sin(Math.toRadians(STEEPEST_DEG)))).toInt()

private fun Offset.clampLength(): Offset {
    val l = getDistance()
    return if (l > 1f) this / l else this
}

/**
 * The top-bar button: a little gimbal whose dot shows which way the paper is
 * tipped, accented while it isn't level.
 */
@Composable
fun GimbalButton(tilt: Offset, selected: Boolean, onClick: () -> Unit) {
    val tipped = tilt != Offset.Zero
    Box(
        Modifier
            .size(44.dp)
            .clip(CircleShape)
            .background(if (selected) AquaColors.Accent.copy(alpha = 0.22f) else Color.Transparent)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.size(22.dp)) {
            val tint = if (selected || tipped) AquaColors.Accent else AquaColors.Text
            drawGimbal(tilt, tint, strokeWidth = 1.6.dp.toPx(), dot = 2.6.dp.toPx(), axes = false)
        }
    }
}

/**
 * A ring that leans with the tilt: a level ring foreshortened along the downhill
 * direction, and a dot that slides downhill.
 */
private fun DrawScope.drawGimbal(tilt: Offset, color: Color, strokeWidth: Float, dot: Float, axes: Boolean) {
    val r = size.minDimension / 2f - strokeWidth
    val c = center
    drawCircle(color.copy(alpha = 0.45f), r, c, style = Stroke(strokeWidth))
    if (axes) {
        val dim = color.copy(alpha = 0.15f)
        drawLine(dim, Offset(c.x - r, c.y), Offset(c.x + r, c.y), strokeWidth / 2f)
        drawLine(dim, Offset(c.x, c.y - r), Offset(c.x, c.y + r), strokeWidth / 2f)
    }
    val m = tilt.getDistance()
    val angle = Math.toDegrees(atan2(tilt.y, tilt.x).toDouble()).toFloat()
    // The paper seen tipped away: squashed along the slope.
    val squash = cos(Math.toRadians(degrees(m).toDouble())).toFloat()
    rotate(angle, c) {
        drawOval(color, Offset(c.x - r * squash, c.y - r), Size(2f * r * squash, 2f * r), style = Stroke(strokeWidth))
    }
    val p = c + tilt * r
    if (m > 0f) drawLine(color.copy(alpha = 0.6f), c, p, strokeWidth)
    drawCircle(color, dot, p)
}

/**
 * The paper-tipping gimbal, shown over the middle of the canvas. Dragging inside
 * it tips the paper towards the finger, and while it is open the phone's own
 * tilt tips it further, measured from how it was held when the gimbal opened or
 * the last drag ended. Tapping outside closes it; the paper stays tipped.
 */
@Composable
fun GimbalOverlay(tilt: Offset, onTilt: (Offset) -> Unit, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val view = LocalView.current
    val tiltNow by rememberUpdatedState(tilt)
    val onTiltNow by rememberUpdatedState(onTilt)
    val gyro = remember { GravityTilt() }

    DisposableEffect(Unit) {
        val sm = context.getSystemService(Context.SENSOR_SERVICE) as SensorManager
        val sensor = sm.getDefaultSensor(Sensor.TYPE_GRAVITY) ?: sm.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
        gyro.base = tiltNow
        val listener = object : SensorEventListener {
            override fun onSensorChanged(e: SensorEvent) {
                gyro.update(e.values, view.display?.rotation ?: Surface.ROTATION_0)?.let { onTiltNow(it) }
            }

            override fun onAccuracyChanged(s: Sensor?, accuracy: Int) {}
        }
        if (sensor != null) sm.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_GAME)
        onDispose { sm.unregisterListener(listener) }
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.25f))
            .pointerInput(Unit) { detectTapGestures { onDismiss() } },
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(
                Modifier
                    .size(220.dp)
                    .clip(CircleShape)
                    .background(AquaColors.Panel)
                    .pointerInput(Unit) {
                        val radius = size.width / 2f * 0.84f
                        fun at(p: Offset): Offset {
                            val t = ((p - Offset(size.width / 2f, size.height / 2f)) / radius).clampLength()
                            return if (t.getDistance() < DEAD_ZONE) Offset.Zero else t
                        }
                        detectDragGestures(
                            onDragStart = { gyro.dragging = true; onTiltNow(at(it)) },
                            onDragEnd = { gyro.release(tiltNow) },
                            onDragCancel = { gyro.release(tiltNow) },
                        ) { change, _ ->
                            change.consume()
                            onTiltNow(at(change.position))
                        }
                    }
                    // Taps inside the gimbal shouldn't close it.
                    .pointerInput(Unit) { detectTapGestures { } },
                contentAlignment = Alignment.Center,
            ) {
                Canvas(Modifier.size(220.dp * 0.84f + 8.dp)) {
                    drawGimbal(tilt, AquaColors.Accent, strokeWidth = 2.dp.toPx(), dot = 9.dp.toPx(), axes = true)
                }
            }
            val m = tilt.getDistance()
            Text(
                if (m == 0f) "Level · drag or tip the phone" else "Tipped ${degrees(m)}°",
                color = AquaColors.Text,
                fontSize = 13.sp,
                modifier = Modifier.clip(RoundedCornerShape(14.dp)).background(AquaColors.Panel).padding(horizontal = 14.dp, vertical = 6.dp),
            )
            Text(
                "Level",
                color = if (m == 0f) AquaColors.TextDim else AquaColors.Accent,
                fontSize = 13.sp,
                modifier = Modifier
                    .clip(RoundedCornerShape(14.dp))
                    .background(AquaColors.Panel)
                    .clickable(enabled = m != 0f) {
                        gyro.release(Offset.Zero)
                        onTiltNow(Offset.Zero)
                    }
                    .padding(horizontal = 14.dp, vertical = 6.dp),
            )
        }
    }
}

/**
 * Turns gravity readings into a screen-space tilt: the paper's tilt is [base]
 * plus however far the phone has tipped since the reference reading, so holding
 * the phone at any comfortable angle starts out from where the paper already is.
 */
private class GravityTilt {
    var base = Offset.Zero
    var dragging = false
    private var ref: FloatArray? = null
    private val g = FloatArray(3)
    private var primed = false

    /** Ends a drag at [t]: the phone's current pose now means [t]. */
    fun release(t: Offset) {
        dragging = false
        base = t
        ref = g.copyOf()
    }

    fun update(v: FloatArray, rotation: Int): Offset? {
        // Device axes into screen axes (x right, y up), for the current display rotation.
        val (sx, sy) = when (rotation) {
            Surface.ROTATION_90 -> -v[1] to v[0]
            Surface.ROTATION_180 -> -v[0] to -v[1]
            Surface.ROTATION_270 -> v[1] to -v[0]
            else -> v[0] to v[1]
        }
        val n = sqrt(sx * sx + sy * sy + v[2] * v[2])
        if (n < 1e-3f) return null
        val now = floatArrayOf(sx / n, sy / n, v[2] / n)
        // Smoothed, so the sensor's noise doesn't shiver the paint.
        if (!primed) {
            now.copyInto(g)
            primed = true
        } else {
            for (i in 0..2) g[i] += (now[i] - g[i]) * 0.2f
        }
        val r = ref ?: g.copyOf().also { ref = it }
        if (dragging) return null
        // Rotate gravity by whatever turns the reference pose flat, so only the
        // change since then counts.
        val d = relative(g, r)
        val steep = sin(Math.toRadians(STEEPEST_DEG)).toFloat()
        // Downhill is against the in-plane part of gravity; screen y points down.
        val t = base + Offset(-d[0], d[1]) / steep
        val out = t.clampLength()
        return if (out.getDistance() < DEAD_ZONE) Offset.Zero else out
    }

    /** [v] expressed in a frame where the reference [r] points straight up (+z). */
    private fun relative(v: FloatArray, r: FloatArray): FloatArray {
        // Axis r × z and angle between them (Rodrigues' rotation).
        val kx = r[1]
        val ky = -r[0]
        val s = hypot(kx, ky)
        val c = r[2]
        if (s < 1e-4f) return if (c > 0f) v else floatArrayOf(v[0], -v[1], -v[2])
        val ux = kx / s
        val uy = ky / s
        // k × v with k = (ux, uy, 0), and k · v.
        val cx = uy * v[2]
        val cy = -ux * v[2]
        val cz = ux * v[1] - uy * v[0]
        val dot = ux * v[0] + uy * v[1]
        return floatArrayOf(
            v[0] * c + cx * s + ux * dot * (1 - c),
            v[1] * c + cy * s + uy * dot * (1 - c),
            v[2] * c + cz * s,
        )
    }
}
