package app.aqua.ui.canvas

import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.aqua.engine.BrushParams
import app.aqua.engine.Tool

@Composable
fun BrushPanel(tool: Tool, params: BrushParams, onChange: (BrushParams) -> Unit, modifier: Modifier = Modifier) {
    FloatingPanel(modifier.width(300.dp)) {
        PanelTitle(
            when (tool) {
                Tool.Watercolor -> "Watercolour brush"
                Tool.Water -> "Clear water"
                Tool.Ink -> "Ink"
                Tool.Eraser -> "Eraser"
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
                LabeledSlider("Pressure → size", params.pressureSize, { onChange(params.copy(pressureSize = it)) })
                LabeledSlider("Smoothing", params.smoothing, { onChange(params.copy(smoothing = it)) })
            }
            Tool.Eraser -> {
                LabeledSlider("Strength", params.strength, { onChange(params.copy(strength = it)) })
                LabeledSlider("Edge softness", params.softness, { onChange(params.copy(softness = it)) }, range = 0.02f..1f)
                LabeledSlider("Pressure → size", params.pressureSize, { onChange(params.copy(pressureSize = it)) })
            }
            Tool.Eyedropper -> {}
        }
    }
}
