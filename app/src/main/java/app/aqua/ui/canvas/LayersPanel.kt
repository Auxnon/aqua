package app.aqua.ui.canvas

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Air
import androidx.compose.material.icons.outlined.ArrowDownward
import androidx.compose.material.icons.outlined.ArrowUpward
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.LayersClear
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material.icons.outlined.WaterDrop
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aqua.engine.CanvasInfo
import app.aqua.engine.LayerOp
import app.aqua.engine.Native
import app.aqua.ui.theme.AquaColors
import kotlin.math.ln
import kotlin.math.pow

@Composable
fun LayersPanel(engine: Long, info: CanvasInfo, onChanged: () -> Unit, modifier: Modifier = Modifier) {
    fun op(op: Int, index: Int, value: Float = 0f, name: String? = null) {
        Native.layerOp(engine, op, index, value, name)
        onChanged()
    }
    val n = info.layers.size
    FloatingPanel(modifier.width(300.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            PanelTitle("Layers", Modifier.weight(1f))
            RoundButton(Icons.Outlined.Add, "Add layer", size = 36.dp, enabled = n < 24) { op(LayerOp.ADD, info.active) }
        }
        LazyColumn(Modifier.heightIn(max = 260.dp).padding(vertical = 6.dp)) {
            // Top of the stack first, like every painting app.
            itemsIndexed(info.layers.asReversed(), key = { _, l -> l.id }) { ri, layer ->
                val index = n - 1 - ri
                val active = index == info.active
                Row(
                    Modifier
                        .fillMaxWidth()
                        .padding(vertical = 2.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(if (active) AquaColors.Accent.copy(alpha = 0.16f) else AquaColors.Raised.copy(alpha = 0.5f))
                        .clickable { op(LayerOp.SELECT, index) }
                        .padding(horizontal = 10.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(layer.name, color = AquaColors.Text, fontSize = 14.sp, modifier = Modifier.weight(1f))
                    if (layer.wet) {
                        Icon(
                            Icons.Outlined.WaterDrop,
                            contentDescription = "Wet",
                            tint = AquaColors.Accent.copy(alpha = (0.35f + layer.wetness.coerceIn(0f, 1f) * 0.65f)),
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(8.dp))
                    }
                    Icon(
                        if (layer.visible) Icons.Outlined.Visibility else Icons.Outlined.VisibilityOff,
                        contentDescription = "Visibility",
                        tint = if (layer.visible) AquaColors.Text else AquaColors.TextDim,
                        modifier = Modifier
                            .size(20.dp)
                            .clickable { op(LayerOp.VISIBLE, index, if (layer.visible) 0f else 1f) },
                    )
                }
            }
        }
        val layer = info.layers.getOrNull(info.active) ?: return@FloatingPanel
        HorizontalDivider(color = AquaColors.Line)
        Spacer(Modifier.size(8.dp))
        // Local slider state so dragging stays smooth between engine round trips.
        var opacity by remember(layer.id) { mutableFloatStateOf(layer.opacity) }
        var drying by remember(layer.id) { mutableFloatStateOf(dryingToSlider(layer.drying)) }
        var flow by remember(layer.id) { mutableFloatStateOf(layer.flow) }
        LabeledSlider("Opacity", opacity, { opacity = it; op(LayerOp.OPACITY, info.active, it) })
        LabeledSlider(
            "Drying speed", drying, { drying = it; op(LayerOp.DRYING_SPEED, info.active, sliderToDrying(it)) },
            format = { formatDrying(sliderToDrying(it)) },
        )
        LabeledSlider(
            "Flood flow  calm → wild", flow, { flow = it; op(LayerOp.FLOW, info.active, it) },
            format = { if (it < 0.02f) "calm" else "${(it * 100).toInt()}%" },
        )
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            RoundButton(Icons.Outlined.Air, "Dry layer now", enabled = layer.wet) { op(LayerOp.DRY, info.active) }
            RoundButton(Icons.Outlined.ArrowUpward, "Move up", enabled = info.active < n - 1) { op(LayerOp.MOVE, info.active, (info.active + 1).toFloat()) }
            RoundButton(Icons.Outlined.ArrowDownward, "Move down", enabled = info.active > 0) { op(LayerOp.MOVE, info.active, (info.active - 1).toFloat()) }
            RoundButton(Icons.Outlined.LayersClear, "Clear layer") { op(LayerOp.CLEAR, info.active) }
            RoundButton(Icons.Outlined.Delete, "Delete layer", enabled = n > 1) { op(LayerOp.REMOVE, info.active) }
        }
    }
}

// Drying speed runs on a log scale from 100x slower than normal (a wash stays
// workable for most of an hour) to 4x faster.
private const val MIN_DRYING = 0.01f
private const val MAX_DRYING = 4f

private fun sliderToDrying(t: Float) = MIN_DRYING * (MAX_DRYING / MIN_DRYING).pow(t)

private fun dryingToSlider(d: Float) = (ln(d.coerceIn(MIN_DRYING, MAX_DRYING) / MIN_DRYING) / ln(MAX_DRYING / MIN_DRYING))

private fun formatDrying(d: Float) = when {
    d >= 0.995f -> "%.1fx".format(d)
    else -> "1/%.0f".format(1f / d)
}
