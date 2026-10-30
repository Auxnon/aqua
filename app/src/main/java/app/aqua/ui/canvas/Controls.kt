package app.aqua.ui.canvas

import app.aqua.engine.Pigment
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aqua.ui.theme.AquaColors

@Composable
fun RoundButton(
    icon: ImageVector,
    description: String,
    modifier: Modifier = Modifier,
    selected: Boolean = false,
    enabled: Boolean = true,
    size: Dp = 44.dp,
    onClick: () -> Unit,
) {
    Box(
        modifier
            .size(size)
            .clip(CircleShape)
            .background(if (selected) AquaColors.Accent.copy(alpha = 0.22f) else Color.Transparent)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = description,
            tint = when {
                !enabled -> AquaColors.TextDim.copy(alpha = 0.35f)
                selected -> AquaColors.Accent
                else -> AquaColors.Text
            },
            modifier = Modifier.size(size * 0.5f),
        )
    }
}

@Composable
fun Swatch(color: Color, modifier: Modifier = Modifier, size: Dp = 34.dp, selected: Boolean = false, ring: Color? = null, onClick: () -> Unit) {
    Box(
        modifier
            .size(size)
            .clip(CircleShape)
            .border(if (selected) 2.dp else 1.dp, if (selected) AquaColors.Text else AquaColors.Line, CircleShape)
            .padding(if (selected) 3.dp else 0.dp)
            .clip(CircleShape)
            .background(color)
            .then(if (ring != null) Modifier.border(2.5.dp, ring, CircleShape) else Modifier)
            .clickable(onClick = onClick),
    )
}

/**
 * A paint's swatch. Paint thinned with water shows the pale tint it lays
 * down, ringed with its full-strength colour so the pigment stays readable.
 */
@Composable
fun PigmentSwatch(p: Pigment, modifier: Modifier = Modifier, size: Dp = 34.dp, selected: Boolean = false, onClick: () -> Unit) {
    Swatch(Color(p.argb), modifier, size, selected, ring = if (p.strength < 0.97f) Color(p.masstoneArgb) else null, onClick = onClick)
}

@Composable
fun FloatingPanel(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier
            .clip(RoundedCornerShape(18.dp))
            .background(AquaColors.Panel)
            .border(1.dp, AquaColors.Line, RoundedCornerShape(18.dp))
            .pointerInput(Unit) { detectTapGestures { } }
            .padding(14.dp),
        content = content,
    )
}

@Composable
fun PanelTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, color = AquaColors.Text, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, modifier = modifier)
}

@Composable
fun LabeledSlider(
    label: String,
    value: Float,
    onChange: (Float) -> Unit,
    range: ClosedFloatingPointRange<Float> = 0f..1f,
    format: (Float) -> String = { "${(it * 100).toInt()}%" },
) {
    Column(Modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text(label, color = AquaColors.TextDim, fontSize = 12.sp)
            Text(format(value), color = AquaColors.TextDim, fontSize = 12.sp)
        }
        Slider(
            value = value,
            onValueChange = onChange,
            valueRange = range,
            modifier = Modifier.height(32.dp),
            colors = SliderDefaults.colors(
                thumbColor = AquaColors.Text,
                activeTrackColor = AquaColors.Accent,
                inactiveTrackColor = AquaColors.Raised,
            ),
        )
    }
}

/**
 * A slim vertical slider in the style of a painting app's side rail. Drags are
 * relative, so grabbing it anywhere never makes the value jump.
 */
@Composable
fun VerticalRailSlider(
    value: Float,
    onChange: (Float) -> Unit,
    modifier: Modifier = Modifier,
    height: Dp = 160.dp,
    label: String,
) {
    val current by rememberUpdatedState(value)
    val onChangeState by rememberUpdatedState(onChange)
    var dragValue by remember { mutableFloatStateOf(value) }
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            Modifier
                .width(40.dp)
                .height(height)
                .pointerInput(Unit) {
                    detectDragGestures(onDragStart = { dragValue = current }) { change, drag ->
                        change.consume()
                        dragValue = (dragValue - drag.y / size.height).coerceIn(0f, 1f)
                        onChangeState(dragValue)
                    }
                }
                .drawBehind {
                    val w = 10.dp.toPx()
                    val x = (size.width - w) / 2f
                    drawRoundRect(AquaColors.Raised, Offset(x, 0f), Size(w, size.height), CornerRadius(w / 2))
                    val y = size.height * (1f - current)
                    drawRoundRect(AquaColors.Accent.copy(alpha = 0.7f), Offset(x, y), Size(w, size.height - y), CornerRadius(w / 2))
                    val th = 24.dp.toPx()
                    drawRoundRect(
                        AquaColors.Text,
                        Offset(size.width / 2f - th / 2f, (y - 5.dp.toPx()).coerceIn(0f, size.height - 10.dp.toPx())),
                        Size(th, 10.dp.toPx()),
                        CornerRadius(5.dp.toPx()),
                    )
                },
        )
        Text(label, color = AquaColors.TextDim, fontSize = 10.sp, modifier = Modifier.padding(top = 4.dp))
    }
}
