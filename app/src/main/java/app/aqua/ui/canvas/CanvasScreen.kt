package app.aqua.ui.canvas

import android.content.ContentValues
import android.graphics.BitmapFactory
import android.content.Context
import android.provider.MediaStore
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.Redo
import androidx.compose.material.icons.automirrored.outlined.Undo
import androidx.compose.material.icons.outlined.Air
import androidx.compose.material.icons.outlined.Brush
import androidx.compose.material.icons.outlined.Colorize
import androidx.compose.material.icons.outlined.HistoryEdu
import androidx.compose.material.icons.outlined.IosShare
import androidx.compose.material.icons.outlined.Layers
import androidx.compose.material.icons.outlined.WaterDrop
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import app.aqua.AquaApp
import app.aqua.engine.BrushParams
import app.aqua.engine.CanvasInfo
import app.aqua.engine.LayerOp
import app.aqua.engine.Native
import app.aqua.engine.Pigment
import app.aqua.engine.Tool
import app.aqua.ui.Arrival
import app.aqua.ui.CanvasFrame
import app.aqua.ui.theme.AquaColors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.math.ln
import kotlin.math.pow

private const val MAX_BRUSH = 300f

private fun sizeToRail(s: Float) = (ln(s.coerceIn(1f, MAX_BRUSH)) / ln(MAX_BRUSH))
private fun railToSize(v: Float) = MAX_BRUSH.pow(v).coerceIn(1f, MAX_BRUSH)

private enum class Panel { None, Layers, Palette, Brush }

@Composable
fun CanvasScreen(dir: File, onExit: (Arrival?) -> Unit) {
    val context = LocalContext.current
    val app = context.applicationContext as AquaApp
    val engine = app.engine
    val scope = rememberCoroutineScope()

    var info by remember { mutableStateOf(CanvasInfo()) }
    var tool by remember { mutableStateOf(Tool.Watercolor) }
    var brushTool by remember { mutableStateOf(Tool.Watercolor) }
    val params = remember {
        mutableStateMapOf(
            Tool.Watercolor to BrushParams(size = 28f),
            Tool.Water to BrushParams(size = 40f, water = 0.8f, softness = 0.3f),
            Tool.Ink to BrushParams(size = 3f, load = 0.8f, softness = 0.05f, bristle = 0f, pressureSize = 0.8f, smoothing = 0.5f, splatter = 0.35f),
            Tool.Eraser to BrushParams(size = 40f, softness = 0.4f, bristle = 0f),
        )
    }
    var paint by remember { mutableStateOf(Pigment.fromArgb(PRESET_PIGMENTS[4].second)) }
    var ink by remember { mutableStateOf(Pigment.fromArgb(0xFF111114.toInt())) }
    var panel by remember { mutableStateOf(Panel.None) }
    var exiting by remember { mutableStateOf(false) }
    var overviewHint by remember { mutableStateOf(false) }
    var surface by remember { mutableStateOf<CanvasSurfaceView?>(null) }
    // Which way the paper is tipped on screen; the gimbal sets it.
    var tilt by remember { mutableStateOf(Offset.Zero) }
    var gimbal by remember { mutableStateOf(false) }
    // The controls fade away while pinching out to the gallery and while leaving.
    val chrome by animateFloatAsState(if (exiting || overviewHint) 0f else 1f, label = "chrome")

    fun refresh() {
        scope.launch(Dispatchers.IO) {
            val i = CanvasInfo.parse(Native.query(engine))
            withContext(Dispatchers.Main) { info = i }
        }
    }

    fun save() = Native.save(engine, dir.path)

    LaunchedEffect(Unit) {
        while (true) {
            refresh()
            delay(500)
        }
    }

    LaunchedEffect(surface, tilt) {
        surface?.setTilt(tilt.x, tilt.y)
    }

    val bp = params[brushTool]!!
    val color = if (brushTool == Tool.Ink) ink else paint
    LaunchedEffect(brushTool, bp, color) {
        Native.setBrush(engine, brushTool.id, bp.toArray(color))
    }

    fun selectTool(t: Tool) {
        if (t == tool && t != Tool.Eyedropper) {
            panel = if (panel == Panel.Brush) Panel.None else Panel.Brush
            return
        }
        tool = t
        if (t != Tool.Eyedropper) brushTool = t
        if (panel == Panel.Brush && t == Tool.Eyedropper) panel = Panel.None
    }

    fun setColor(p: Pigment) {
        if (brushTool == Tool.Ink) ink = p else paint = p
        if (brushTool == Tool.Eraser || brushTool == Tool.Water) {
            paint = p
            brushTool = Tool.Watercolor
            tool = Tool.Watercolor
        }
    }

    /** Saves and goes back to the gallery, which flies the painting from [from] into its card. */
    fun leave(from: CanvasFrame? = surface?.frame()) {
        if (exiting) return
        exiting = true
        panel = Panel.None
        // Not cancellable: this screen leaves composition partway through.
        scope.launch(Dispatchers.IO + NonCancellable) {
            save()
            // Decoded here so the gallery can show it on its very first frame.
            val thumb = from?.let { BitmapFactory.decodeFile(File(dir, "thumb.png").path)?.asImageBitmap() }
            val arrival = if (from != null && thumb != null) Arrival(dir.path, from, thumb) else null
            withContext(Dispatchers.Main) { onExit(arrival) }
            // Only once the gallery has taken over: closing earlier blanks the
            // canvas surface while it's still on screen.
            Native.closeCanvas(engine)
        }
    }

    BackHandler {
        when {
            gimbal -> gimbal = false
            panel != Panel.None -> panel = Panel.None
            else -> leave()
        }
    }

    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle) {
        val obs = LifecycleEventObserver { _, e ->
            if (e == Lifecycle.Event.ON_STOP && !exiting) scope.launch(Dispatchers.IO) { save() }
        }
        lifecycle.addObserver(obs)
        onDispose { lifecycle.removeObserver(obs) }
    }

    Box(Modifier.fillMaxSize().background(AquaColors.Background)) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { ctx ->
                CanvasSurfaceView(ctx, engine).apply {
                    onStrokeEnd = { refresh() }
                    onUndo = { Native.undo(engine); refresh() }
                    onRedo = { Native.redo(engine); refresh() }
                    onOverviewHint = { overviewHint = it }
                    onOverview = { leave(it) }
                }.also { surface = it }
            },
            update = { v ->
                v.eyedropper = tool == Tool.Eyedropper
                v.onSample = { rgb -> setColor(Pigment.fromRgb(rgb[0], rgb[1], rgb[2])) }
                v.onTouchStart = { panel = Panel.None }
                v.setCanvasSize(info.width, info.height)
            },
        )

        Box(Modifier.fillMaxSize().graphicsLayer { alpha = chrome }) {
            // Top bar.
            Row(
                Modifier.fillMaxWidth().statusBarsPadding().padding(horizontal = 8.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Bar {
                    RoundButton(Icons.AutoMirrored.Outlined.ArrowBack, "Gallery") { leave() }
                    RoundButton(Icons.AutoMirrored.Outlined.Undo, "Undo", enabled = info.canUndo) { Native.undo(engine); refresh() }
                    RoundButton(Icons.AutoMirrored.Outlined.Redo, "Redo", enabled = info.canRedo) { Native.redo(engine); refresh() }
                }
                Spacer(Modifier.weight(1f))
                Bar {
                    val activeWet = info.layers.getOrNull(info.active)?.wet == true
                    AnimatedVisibility(activeWet, enter = fadeIn(), exit = fadeOut()) {
                        RoundButton(Icons.Outlined.Air, "Dry layer now") {
                            Native.layerOp(engine, LayerOp.DRY, info.active, 0f, null)
                            refresh()
                        }
                    }
                    GimbalButton(tilt, selected = gimbal) {
                        panel = Panel.None
                        gimbal = !gimbal
                    }
                    RoundButton(Icons.Outlined.IosShare, "Export") {
                        scope.launch {
                            val ok = exportToGallery(context, engine)
                            Toast.makeText(context, if (ok) "Saved to Pictures/Aqua" else "Export failed", Toast.LENGTH_SHORT).show()
                        }
                    }
                    RoundButton(Icons.Outlined.Layers, "Layers", selected = panel == Panel.Layers) {
                        panel = if (panel == Panel.Layers) Panel.None else Panel.Layers
                    }
                    Swatch(Color(color.argb), Modifier.padding(horizontal = 6.dp), size = 32.dp, selected = panel == Panel.Palette) {
                        panel = if (panel == Panel.Palette) Panel.None else Panel.Palette
                    }
                }
            }

            // Side rail: size and the tool's main wetness / strength control.
            Column(
                Modifier
                    .align(Alignment.CenterStart)
                    .padding(start = 6.dp)
                    .clip(RoundedCornerShape(16.dp))
                    .background(AquaColors.Panel)
                    .padding(vertical = 10.dp, horizontal = 2.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                VerticalRailSlider(sizeToRail(bp.size), { params[brushTool] = bp.copy(size = railToSize(it)) }, label = "${bp.size.toInt()}")
                Spacer(Modifier.size(14.dp))
                when (brushTool) {
                    Tool.Watercolor, Tool.Water -> VerticalRailSlider(bp.water, { params[brushTool] = bp.copy(water = it) }, height = 120.dp, label = "water")
                    Tool.Ink -> VerticalRailSlider(bp.bleed, { params[brushTool] = bp.copy(bleed = it) }, height = 120.dp, label = "bleed")
                    Tool.Eraser -> VerticalRailSlider(bp.strength, { params[brushTool] = bp.copy(strength = it) }, height = 120.dp, label = "strength")
                    Tool.Eyedropper -> {}
                }
            }

            // Tool dock.
            Row(
                Modifier
                    .align(Alignment.BottomCenter)
                    .navigationBarsPadding()
                    .padding(bottom = 12.dp)
                    .clip(RoundedCornerShape(28.dp))
                    .background(AquaColors.Panel)
                    .padding(horizontal = 8.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                RoundButton(Icons.Outlined.Brush, "Watercolour", selected = tool == Tool.Watercolor, size = 48.dp) { selectTool(Tool.Watercolor) }
                RoundButton(Icons.Outlined.WaterDrop, "Clear water", selected = tool == Tool.Water, size = 48.dp) { selectTool(Tool.Water) }
                RoundButton(Icons.Outlined.HistoryEdu, "Ink", selected = tool == Tool.Ink, size = 48.dp) { selectTool(Tool.Ink) }
                RoundButton(EraserIcon, "Eraser", selected = tool == Tool.Eraser, size = 48.dp) { selectTool(Tool.Eraser) }
                RoundButton(Icons.Outlined.Colorize, "Eyedropper", selected = tool == Tool.Eyedropper, size = 48.dp) { selectTool(Tool.Eyedropper) }
            }

            // Panels.
            val panelMod = Modifier.statusBarsPadding().padding(top = 56.dp, end = 10.dp)
            when (panel) {
                Panel.Layers -> LayersPanel(engine, info, { refresh() }, panelMod.align(Alignment.TopEnd))
                Panel.Palette -> PalettePanel(color, { setColor(it) }, panelMod.align(Alignment.TopEnd))
                Panel.Brush -> BrushPanel(
                    brushTool, bp, { params[brushTool] = it },
                    Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 80.dp),
                )
                Panel.None -> {}
            }
        }

        AnimatedVisibility(gimbal && !exiting, enter = fadeIn(), exit = fadeOut()) {
            GimbalOverlay(tilt, { tilt = it }, onDismiss = { gimbal = false })
        }

        AnimatedVisibility(
            overviewHint && !exiting,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 36.dp),
        ) {
            Text(
                "Release for gallery",
                color = AquaColors.Text,
                fontSize = 14.sp,
                modifier = Modifier.clip(RoundedCornerShape(16.dp)).background(AquaColors.Panel).padding(horizontal = 16.dp, vertical = 8.dp),
            )
        }
    }
}

@Composable
private fun Bar(content: @Composable () -> Unit) {
    Row(
        Modifier.clip(RoundedCornerShape(24.dp)).background(AquaColors.Panel).padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) { content() }
}

private suspend fun exportToGallery(context: Context, engine: Long): Boolean = withContext(Dispatchers.IO) {
    val tmp = File(context.cacheDir, "export.png")
    if (!Native.exportPng(engine, tmp.path)) return@withContext false
    val values = ContentValues().apply {
        put(MediaStore.Images.Media.DISPLAY_NAME, "Aqua_${System.currentTimeMillis()}.png")
        put(MediaStore.Images.Media.MIME_TYPE, "image/png")
        put(MediaStore.Images.Media.RELATIVE_PATH, "Pictures/Aqua")
    }
    val resolver = context.contentResolver
    val uri = resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values) ?: return@withContext false
    runCatching {
        resolver.openOutputStream(uri)?.use { out -> tmp.inputStream().use { it.copyTo(out) } }
        tmp.delete()
    }.isSuccess
}
