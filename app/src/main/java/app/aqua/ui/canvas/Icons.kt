package app.aqua.ui.canvas

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

/** Material has no eraser glyph; a simple tilted block in the same outlined style. */
val EraserIcon: ImageVector = ImageVector.Builder("Eraser", 24.dp, 24.dp, 24f, 24f).apply {
    path(stroke = SolidColor(Color.Black), strokeLineWidth = 1.8f, strokeLineJoin = StrokeJoin.Round) {
        moveTo(14.5f, 4.5f)
        lineTo(20f, 10f)
        lineTo(11f, 19f)
        lineTo(7f, 19f)
        lineTo(3.5f, 15.5f)
        close()
        moveTo(9f, 10f)
        lineTo(14.5f, 15.5f)
        moveTo(11f, 19f)
        lineTo(20.5f, 19f)
    }
}.build()
