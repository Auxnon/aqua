package app.aqua.ui.canvas

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aqua.AquaApp
import app.aqua.engine.Native
import app.aqua.engine.Pigment
import app.aqua.ui.theme.AquaColors
import org.json.JSONArray

/** Masstones of the preset pigments (matches engine/src/pigment.rs). */
val PRESET_PIGMENTS: List<Pair<String, Int>> = listOf(
    "Hansa Yellow" to 0xFFFACC14.toInt(),
    "New Gamboge" to 0xFFF5990D.toInt(),
    "Pyrrol Scarlet" to 0xFFDB291A.toInt(),
    "Quinacridone Rose" to 0xFFCC1F61.toInt(),
    "Ultramarine" to 0xFF29339E.toInt(),
    "Phthalo Blue" to 0xFF083880.toInt(),
    "Phthalo Green" to 0xFF006654.toInt(),
    "Burnt Sienna" to 0xFF8C3D1F.toInt(),
    "Raw Umber" to 0xFF664F33.toInt(),
    "Payne's Grey" to 0xFF38404D.toInt(),
    "Indigo" to 0xFF262942.toInt(),
    "Lamp Black" to 0xFF121212.toInt(),
)

private enum class TrayMode(val label: String) { Paint("Paint"), Mix("Mix"), Pick("Pick") }

/** Mixed colours the user picked from the tray, remembered across sessions. */
class SwatchStore(context: Context) {
    private val prefs = context.getSharedPreferences("swatches", Context.MODE_PRIVATE)

    fun load(): List<Pigment> = runCatching {
        val a = JSONArray(prefs.getString("list", "[]"))
        (0 until a.length()).map { i ->
            val k = a.getJSONArray(i)
            Pigment(FloatArray(4) { k.getDouble(it).toFloat() })
        }
    }.getOrDefault(emptyList())

    fun save(list: List<Pigment>) {
        val a = JSONArray()
        list.forEach { p -> a.put(JSONArray().apply { p.k.forEach { put(it.toDouble()) } }) }
        prefs.edit().putString("list", a.toString()).apply()
    }
}

/**
 * A Paper-style mixing palette: dab pigments into the tray, swirl them together
 * with a finger, then pick the mixture up as the brush colour. The tray mixes in
 * Kubelka-Munk space, so what you mix is what the canvas will paint.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun PalettePanel(current: Pigment, onPick: (Pigment) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val app = context.applicationContext as AquaApp
    val traySize = AquaApp.PALETTE_SIZE
    val store = remember { SwatchStore(context) }
    val presets = remember { PRESET_PIGMENTS.map { Pigment.fromArgb(it.second) } }
    val swatches = remember { mutableStateListOf<Pigment>().apply { addAll(store.load()) } }
    var mode by remember { mutableStateOf(TrayMode.Paint) }
    var well by remember { mutableStateOf(current) }
    var showPicker by remember { mutableStateOf(false) }
    val bitmap = remember { Bitmap.createBitmap(traySize, traySize, Bitmap.Config.ARGB_8888) }
    val pixels = remember { IntArray(traySize * traySize) }
    var version by remember { mutableIntStateOf(0) }

    fun redraw() {
        Native.paletteRender(app.palette, pixels)
        bitmap.setPixels(pixels, 0, traySize, 0, 0, traySize, traySize)
        version++
    }
    LaunchedEffect(Unit) { redraw() }

    fun pick(p: Pigment) {
        onPick(p)
        swatches.remove(p)
        swatches.add(0, p)
        while (swatches.size > 16) swatches.removeAt(swatches.lastIndex)
        store.save(swatches)
    }

    FloatingPanel(modifier.width(320.dp)) {
        PanelTitle("Palette")
        Spacer(Modifier.height(8.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            presets.forEach { p ->
                PigmentSwatch(p, size = 30.dp, selected = p == well) {
                    well = p
                    if (mode == TrayMode.Pick) mode = TrayMode.Paint
                    onPick(p)
                }
            }
            Box(
                Modifier
                    .size(30.dp)
                    .clip(RoundedCornerShape(15.dp))
                    .border(1.dp, AquaColors.Line, RoundedCornerShape(15.dp))
                    .clickable { showPicker = !showPicker },
                contentAlignment = Alignment.Center,
            ) { androidx.compose.material3.Icon(Icons.Outlined.Add, "Custom colour", tint = AquaColors.Text, modifier = Modifier.size(18.dp)) }
        }
        if (showPicker) {
            ColorSliders(initial = well) { p ->
                well = p
                onPick(p)
            }
        }
        Spacer(Modifier.height(10.dp))

        // The mixing tray.
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(1f)
                .clip(RoundedCornerShape(14.dp))
                .background(Color(0xFFE6E4DF))
                .pointerInput(mode, well) {
                    awaitEachGesture {
                        val down = awaitFirstDown()
                        val scale = traySize / size.width.toFloat()
                        var px = down.position.x * scale
                        var py = down.position.y * scale
                        when (mode) {
                            TrayMode.Paint -> Native.paletteDab(app.palette, px, py, 10f, well.k, 0.35f)
                            TrayMode.Mix -> Native.paletteSmudgeBegin(app.palette, px, py, 14f)
                            TrayMode.Pick -> Native.palettePick(app.palette, px, py, 4f)?.let { pick(Pigment(it)) }
                        }
                        redraw()
                        while (true) {
                            val ev = awaitPointerEvent()
                            val c = ev.changes.firstOrNull { it.id == down.id } ?: break
                            if (!c.pressed) break
                            val nx = c.position.x * scale
                            val ny = c.position.y * scale
                            when (mode) {
                                TrayMode.Paint -> {
                                    val d = kotlin.math.hypot(nx - px, ny - py)
                                    val steps = (d / 3f).toInt().coerceAtLeast(1)
                                    for (s in 1..steps) {
                                        val t = s / steps.toFloat()
                                        Native.paletteDab(app.palette, px + (nx - px) * t, py + (ny - py) * t, 10f, well.k, 0.12f)
                                    }
                                }
                                TrayMode.Mix -> Native.paletteSmudge(app.palette, px, py, nx, ny, 0.9f)
                                TrayMode.Pick -> Native.palettePick(app.palette, nx, ny, 4f)?.let { onPick(Pigment(it)) }
                            }
                            px = nx
                            py = ny
                            c.consume()
                            redraw()
                        }
                        if (mode == TrayMode.Mix) Native.paletteSmudgeEnd(app.palette)
                        if (mode == TrayMode.Pick) Native.palettePick(app.palette, px, py, 4f)?.let { pick(Pigment(it)) }
                        app.savePalette()
                    }
                },
        ) {
            val image = remember { bitmap.asImageBitmap() }
            Spacer(
                Modifier
                    .fillMaxWidth()
                    .aspectRatio(1f)
                    .drawBehind {
                        // Reading `version` re-runs this draw after every tray change.
                        if (version >= 0) {
                            drawImage(
                                image,
                                dstSize = IntSize(size.width.toInt(), size.height.toInt()),
                                filterQuality = FilterQuality.Low,
                            )
                        }
                    },
            )
        }
        Spacer(Modifier.height(8.dp))
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            TrayMode.entries.forEach { m ->
                val sel = m == mode
                Text(
                    m.label,
                    color = if (sel) AquaColors.Background else AquaColors.Text,
                    fontSize = 13.sp,
                    modifier = Modifier
                        .padding(end = 6.dp)
                        .clip(RoundedCornerShape(14.dp))
                        .background(if (sel) AquaColors.Accent else AquaColors.Raised)
                        .clickable { mode = m }
                        .padding(horizontal = 12.dp, vertical = 6.dp),
                )
            }
            Spacer(Modifier.weight(1f))
            RoundButton(Icons.Outlined.Delete, "Clear tray", size = 36.dp) {
                Native.paletteClear(app.palette)
                redraw()
                app.savePalette()
            }
        }
        if (swatches.isNotEmpty()) {
            Spacer(Modifier.height(8.dp))
            Text("Mixed", color = AquaColors.TextDim, fontSize = 12.sp)
            Spacer(Modifier.height(4.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                swatches.forEach { p -> PigmentSwatch(p, size = 26.dp, selected = p == current) { well = p; onPick(p) } }
            }
        }
    }
}

/** Minimal HSV picker for colours that aren't in the preset row. */
@Composable
private fun ColorSliders(initial: Pigment, onChange: (Pigment) -> Unit) {
    val hsv = remember { FloatArray(3).also { android.graphics.Color.colorToHSV(initial.masstoneArgb, it) } }
    var h by remember { mutableStateOf(hsv[0]) }
    var s by remember { mutableStateOf(hsv[1]) }
    var v by remember { mutableStateOf(hsv[2]) }
    fun emit() = onChange(Pigment.fromArgb(android.graphics.Color.HSVToColor(floatArrayOf(h, s, v))))
    Column(Modifier.padding(top = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(28.dp).clip(RoundedCornerShape(14.dp)).background(Color(android.graphics.Color.HSVToColor(floatArrayOf(h, s, v)))))
            Spacer(Modifier.width(8.dp))
            Text("Custom colour", color = AquaColors.TextDim, fontSize = 12.sp)
        }
        LabeledSlider("Hue", h, { h = it; emit() }, range = 0f..360f, format = { "${it.toInt()}°" })
        LabeledSlider("Saturation", s, { s = it; emit() })
        LabeledSlider("Value", v, { v = it; emit() })
    }
}
