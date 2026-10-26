package app.aqua.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

object AquaColors {
    val Background = Color(0xFF09090B)
    val Panel = Color(0xE61A1A1E)
    val PanelSolid = Color(0xFF1A1A1E)
    val Raised = Color(0xFF2A2A30)
    val Line = Color(0x33FFFFFF)
    val Text = Color(0xFFECEAE6)
    val TextDim = Color(0xFF9A9894)
    val Accent = Color(0xFF8FD3CB)
    val Danger = Color(0xFFE5806F)
}

@Composable
fun AquaTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = AquaColors.Accent,
            onPrimary = Color(0xFF0C2B28),
            background = AquaColors.Background,
            surface = AquaColors.PanelSolid,
            surfaceVariant = AquaColors.Raised,
            onSurface = AquaColors.Text,
            onSurfaceVariant = AquaColors.TextDim,
            outline = AquaColors.Line,
            error = AquaColors.Danger,
        ),
        content = content,
    )
}
