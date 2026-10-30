package app.aqua.ui.canvas

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Colorize
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aqua.data.PickKind
import app.aqua.engine.BrushParams
import app.aqua.engine.Tool
import app.aqua.ui.theme.AquaColors

@Composable
fun BrushPanel(tool: Tool, params: BrushParams, onChange: (BrushParams) -> Unit, modifier: Modifier = Modifier) {
    FloatingPanel(modifier.width(300.dp)) {
        PanelTitle(
            when (tool) {
                Tool.Watercolor -> "Watercolour brush"
                Tool.Water -> "Clear water"
                Tool.Ink -> "Ink"
                Tool.Eraser -> "Eraser"
                Tool.Push -> "Push"
                Tool.Eyedropper -> "Eyedropper"
            },
        )
        when (tool) {
            Tool.Watercolor -> {
                LabeledSlider("Water", params.water, { onChange(params.copy(water = it)) })
                LabeledSlider("Pigment", params.load, { onChange(params.copy(load = it)) })
                LabeledSlider("Edge softness", params.softness, { onChange(params.copy(softness = it)) }, range = 0.02f..1f)
                LabeledSlider("Bristles", params.bristle, { onChange(params.copy(bristle = it)) })
                LabeledSlider("Pressure → size", params.pressureSize, { onChange(params.copy(pressureSize = it)) })
                LabeledSlider("Pressure → flow", params.pressureFlow, { onChange(params.copy(pressureFlow = it)) })
                LabeledSlider("Smoothing", params.smoothing, { onChange(params.copy(smoothing = it)) })
            }
            Tool.Water -> {
                LabeledSlider("Water", params.water, { onChange(params.copy(water = it)) })
                LabeledSlider("Edge softness", params.softness, { onChange(params.copy(softness = it)) }, range = 0.02f..1f)
                LabeledSlider("Pressure → size", params.pressureSize, { onChange(params.copy(pressureSize = it)) })
                LabeledSlider("Smoothing", params.smoothing, { onChange(params.copy(smoothing = it)) })
            }
            Tool.Ink -> {
                LabeledSlider(
                    "Pen  ←  bleed  →  India ink", params.bleed, { onChange(params.copy(bleed = it)) },
                    format = { if (it < 0.02f) "pen" else "${(it * 100).toInt()}%" },
                )
                LabeledSlider("Density", params.load, { onChange(params.copy(load = it)) })
                LabeledSlider(
                    "Splatter (quick flicks)", params.splatter, { onChange(params.copy(splatter = it)) },
                    format = { if (it < 0.02f) "off" else "${(it * 100).toInt()}%" },
                )
                LabeledSlider("Pressure → size", params.pressureSize, { onChange(params.copy(pressureSize = it)) })
                LabeledSlider("Smoothing", params.smoothing, { onChange(params.copy(smoothing = it)) })
            }
            Tool.Eraser -> {
                LabeledSlider("Strength", params.strength, { onChange(params.copy(strength = it)) })
                LabeledSlider("Edge softness", params.softness, { onChange(params.copy(softness = it)) }, range = 0.02f..1f)
                LabeledSlider("Pressure → size", params.pressureSize, { onChange(params.copy(pressureSize = it)) })
            }
            Tool.Push -> {
                LabeledSlider("Strength", params.strength, { onChange(params.copy(strength = it)) })
                LabeledSlider("Edge softness", params.softness, { onChange(params.copy(softness = it)) }, range = 0.02f..1f)
                LabeledSlider("Pressure → size", params.pressureSize, { onChange(params.copy(pressureSize = it)) })
                LabeledSlider("Smoothing", params.smoothing, { onChange(params.copy(smoothing = it)) })
                Text("Pushes standing water and the paint in it. Does nothing on dry paper.", color = AquaColors.TextDim, fontSize = 12.sp)
            }
            Tool.Eyedropper -> {}
        }
    }
}

/** The eyedropper's two kinds: read the canvas as a wash, or as an exact colour. */
@Composable
fun PickerPanel(kind: PickKind, onChange: (PickKind) -> Unit, modifier: Modifier = Modifier) {
    FloatingPanel(modifier.width(300.dp)) {
        PanelTitle("Eyedropper")
        Spacer(Modifier.height(6.dp))
        PickerChoice(WashPickIcon, "Wash", "Whiter areas pick up thinner paint of a deeper pigment", kind == PickKind.Wash) { onChange(PickKind.Wash) }
        PickerChoice(Icons.Outlined.Colorize, "Classic", "The exact colour, as full-strength paint", kind == PickKind.Classic) { onChange(PickKind.Classic) }
    }
}

@Composable
private fun PickerChoice(icon: ImageVector, title: String, detail: String, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(if (selected) AquaColors.Raised else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = if (selected) AquaColors.Accent else AquaColors.Text, modifier = Modifier.size(22.dp))
        Spacer(Modifier.width(12.dp))
        Column {
            Text(title, color = if (selected) AquaColors.Accent else AquaColors.Text, fontSize = 14.sp)
            Text(detail, color = AquaColors.TextDim, fontSize = 12.sp)
        }
    }
}
