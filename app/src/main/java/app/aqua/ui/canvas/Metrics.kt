package app.aqua.ui.canvas

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * How big the painting controls are. A tablet (smallest width of at least
 * 600dp, e.g. a Galaxy Tab S at about 820 × 1300dp) gets bigger buttons, longer
 * rail sliders and roomier panels: it is held further away, often painted on
 * with a pen while the other hand reaches for the controls.
 */
@Immutable
data class Metrics(
    val tablet: Boolean,
    /** Top bar buttons. */
    val bar: Dp,
    /** Tool dock buttons. */
    val dock: Dp,
    /** The rail's size slider, and its second slider. */
    val railSize: Dp,
    val railSecond: Dp,
    /** Added to every floating panel's width. */
    val panelExtra: Dp,
    /** How tall the layer list grows before it scrolls. */
    val layerList: Dp,
    /** The water cup / ink well. */
    val cup: Dp,
) {
    fun panel(width: Dp) = width + panelExtra

    companion object {
        val Phone = Metrics(false, 44.dp, 48.dp, 160.dp, 120.dp, 0.dp, 260.dp, 60.dp)
        val Tablet = Metrics(true, 48.dp, 56.dp, 260.dp, 190.dp, 72.dp, 480.dp, 84.dp)
    }
}

val LocalMetrics = staticCompositionLocalOf { Metrics.Phone }

/** The metrics for this window (a split-screen half of a tablet counts as what it is). */
@Composable
fun windowMetrics(): Metrics {
    return if (LocalConfiguration.current.smallestScreenWidthDp >= 600) Metrics.Tablet else Metrics.Phone
}
