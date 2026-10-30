package app.aqua.ui.canvas

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
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

/** The push brush: a wave of water shoved along by an arrow. */
val PushIcon: ImageVector = ImageVector.Builder("Push", 24.dp, 24.dp, 24f, 24f).apply {
    path(stroke = SolidColor(Color.Black), strokeLineWidth = 1.8f, strokeLineJoin = StrokeJoin.Round, strokeLineCap = StrokeCap.Round) {
        moveTo(3f, 15f)
        curveTo(5f, 13f, 7f, 13f, 9f, 15f)
        curveTo(11f, 17f, 13f, 17f, 15f, 15f)
        moveTo(3f, 19.5f)
        curveTo(5f, 17.5f, 7f, 17.5f, 9f, 19.5f)
        curveTo(11f, 21.5f, 13f, 21.5f, 15f, 19.5f)
        moveTo(11f, 8f)
        lineTo(20f, 8f)
        moveTo(17f, 5f)
        lineTo(20f, 8f)
        lineTo(17f, 11f)
    }
}.build()

/** The wash eyedropper: a pipette over a drop, for picking paint by how thin it lies. */
val WashPickIcon: ImageVector = ImageVector.Builder("WashPick", 24.dp, 24.dp, 24f, 24f).apply {
    path(stroke = SolidColor(Color.Black), strokeLineWidth = 1.8f, strokeLineJoin = StrokeJoin.Round, strokeLineCap = StrokeCap.Round) {
        // Pipette, tip at the lower left.
        moveTo(16.5f, 3.5f)
        lineTo(20.5f, 7.5f)
        moveTo(18.5f, 5.5f)
        lineTo(9f, 15f)
        lineTo(6.5f, 15.5f)
        lineTo(7f, 13f)
        lineTo(16.5f, 3.5f)
        // Drop.
        moveTo(5f, 17f)
        curveTo(5f, 17f, 3f, 19.2f, 3f, 20.3f)
        curveTo(3f, 21.4f, 3.9f, 22f, 5f, 22f)
        curveTo(6.1f, 22f, 7f, 21.4f, 7f, 20.3f)
        curveTo(7f, 19.2f, 5f, 17f, 5f, 17f)
        close()
    }
}.build()
