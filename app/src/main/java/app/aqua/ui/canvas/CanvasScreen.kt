package app.aqua.ui.canvas

import android.app.Activity
import android.content.ContentValues
import android.graphics.BitmapFactory
import android.content.Context
import android.provider.MediaStore
import android.view.WindowManager
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.material.icons.outlined.AllInclusive
import androidx.compose.material.icons.outlined.Brush
import androidx.compose.material.icons.outlined.Colorize
import androidx.compose.material.icons.outlined.HistoryEdu
import androidx.compose.material.icons.outlined.IosShare
import androidx.compose.material.icons.outlined.Layers
import androidx.compose.material.icons.outlined.Sync
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material.icons.outlined.WaterDrop
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
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
import androidx.lifecycle.compose.currentStateAsState
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import app.aqua.AquaApp
import app.aqua.data.PickKind
import app.aqua.data.PinchOut
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

/** With sync on, a painting is saved (and pushed) once it has been left alone this long. */
private const val SYNC_IDLE_MS = 20_000L

private fun sizeToRail(s: Float) = (ln(s.coerceIn(1f, MAX_BRUSH)) / ln(MAX_BRUSH))
private fun railToSize(v: Float) = MAX_BRUSH.pow(v).coerceIn(1f, MAX_BRUSH)

private enum class Panel { None, Layers, Palette, Brush, Picker }

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
            Tool.Ink to BrushParams(size = 3f, load = 0.8f, softness = 0.05f, bristle = 0f, pressureSize = 0.8f, smoothing = 0.5f, splatter = 0.35f, tilt = 0.25f),
            Tool.Eraser to BrushParams(size = 40f, softness = 0.4f, bristle = 0f),
            Tool.Push to BrushParams(size = 36f, softness = 0.4f, bristle = 0f),
        )
    }
    var paint by remember { mutableStateOf(Pigment.fromArgb(PRESET_PIGMENTS[4].second)) }
    var ink by remember { mutableStateOf(Pigment.fromArgb(0xFF111114.toInt())) }
    var panel by remember { mutableStateOf(Panel.None) }
    var exiting by remember { mutableStateOf(false) }
    var overviewHint by remember { mutableStateOf(false) }
    val pinchToGallery = remember { app.settings.pinchOut == PinchOut.Gallery }
    var pickKind by remember { mutableStateOf(app.settings.pickKind) }
    // The brush runs out of water and has to be dipped in the cup.
    var dip by remember { mutableStateOf(app.settings.dip) }
    var surface by remember { mutableStateOf<CanvasSurfaceView?>(null) }
    // Which way the paper is tipped on screen; the gimbal sets it.
    var tilt by remember { mutableStateOf(Offset.Zero) }
    var gimbal by remember { mutableStateOf(false) }
    val gyro = remember { GravityTilt() }
    val canKeepTilt = remember { hasTiltSensor(context) }
    // Phone tilt keeps steering the paper after the gimbal is closed.
    var keepTilt by remember { mutableStateOf(false) }
    // Every control but one button to bring them back.
    var hidden by remember { mutableStateOf(false) }
    var syncOn by remember { mutableStateOf(app.settings.syncOn) }
    var syncDialog by remember { mutableStateOf(false) }
    val syncStatus by app.sync.status.collectAsState()
    // The revision last saved, so sync only pushes what changed.
    var savedRevision by remember { mutableStateOf<Long?>(null) }
    // The controls fade away while pinching out to the gallery and while leaving.
    val chrome by animateFloatAsState(if (exiting || (overviewHint && pinchToGallery)) 0f else 1f, label = "chrome")

    fun refresh() {
        scope.launch(Dispatchers.IO) {
            val i = CanvasInfo.parse(Native.query(engine))
            withContext(Dispatchers.Main) { info = i }
        }
    }

    /** Saves, and pushes the save to the sync daemons when sync is on. */
    fun save(force: Boolean = false): Boolean {
        val rev = info.revision
        val ok = Native.save(engine, dir.path)
        if (ok) {
            savedRevision = rev
            app.sync.push(dir, force)
        }
        return ok
    }

    // With sync on, a painting left alone for a while is saved and pushed, not
    // only when leaving it.
    LaunchedEffect(info.revision, info.width, syncOn) {
        if (info.width == 0) return@LaunchedEffect
        if (savedRevision == null) savedRevision = info.revision
        if (!syncOn || info.revision == savedRevision) return@LaunchedEffect
        delay(SYNC_IDLE_MS)
        withContext(Dispatchers.IO) { save() }
    }

    LaunchedEffect(Unit) {
        while (true) {
            refresh()
            delay(500)
        }
    }

    // The paper turns back against the screen when the device turns, so the
    // system's own rotation animation would turn it twice. Seamless rotation
    // (made for camera viewfinders) skips that, and only the controls move.
    DisposableEffect(Unit) {
        val window = (context as? Activity)?.window
        val before = window?.attributes?.rotationAnimation
        window?.attributes = window.attributes.apply {
            rotationAnimation = WindowManager.LayoutParams.ROTATION_ANIMATION_SEAMLESS
        }
        onDispose {
            if (window != null && before != null) window.attributes = window.attributes.apply { rotationAnimation = before }
        }
    }

    LaunchedEffect(surface, tilt) {
        surface?.setTilt(tilt.x, tilt.y)
    }

    val bp = params[brushTool]!!
    val color = if (brushTool == Tool.Ink) ink else paint
    // True while a pen is turned round to its eraser end.
    var penEraser by remember { mutableStateOf(false) }
    fun strokeTool() = if (penEraser) Tool.Eraser else brushTool
    fun applyBrush() {
        val t = strokeTool()
        Native.setBrush(engine, t.id, params[t]!!.toArray(if (brushTool == Tool.Ink) ink else paint))
    }
    LaunchedEffect(brushTool, bp, color, penEraser) { applyBrush() }

    fun selectTool(t: Tool) {
        if (t == tool) {
            // Tapping the selected tool again opens its panel: brush settings, or
            // for the eyedropper which kind of picker it is.
            val own = if (t == Tool.Eyedropper) Panel.Picker else Panel.Brush
            panel = if (panel == own) Panel.None else own
            return
        }
        tool = t
        if (t != Tool.Eyedropper) brushTool = t
        if (panel == Panel.Brush || panel == Panel.Picker) panel = Panel.None
    }

    LaunchedEffect(dip) {
        // A fresh brush (and pen) starts full.
        Native.setCharge(engine, Tool.Watercolor.id, if (dip) 1f else -1f)
        info = info.copy(charge = if (dip) 1f else null, inkCharge = 1f)
        // Dipping is how the brush gets its water, and a dry brush pushes wet
        // paint by itself, so those two tools aren't needed.
        if (dip && (tool == Tool.Water || tool == Tool.Push)) {
            tool = Tool.Watercolor
            brushTool = Tool.Watercolor
            if (panel == Panel.Brush) panel = Panel.None
        }
    }

    /** Dips the pen in the ink well, or any other brush in the water cup. */
    fun dipBrush() {
        if (brushTool == Tool.Ink) {
            Native.setCharge(engine, Tool.Ink.id, 1f)
            info = info.copy(inkCharge = 1f)
        } else {
            Native.setCharge(engine, Tool.Watercolor.id, 1f)
            info = info.copy(charge = 1f)
        }
    }

    fun setColor(p: Pigment) {
        if (brushTool == Tool.Ink) ink = p else paint = p
        if (brushTool == Tool.Eraser || brushTool == Tool.Water || brushTool == Tool.Push) {
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
            hidden -> hidden = false
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
                    pinchOutToGallery = pinchToGallery
                    onOverview = { leave(it) }
                    onTiltTurned = { x, y ->
                        tilt = Offset(x, y)
                        // Phone tilt is read in screen axes, which just turned; start again from here.
                        gyro.start(tilt)
                    }
                }.also { surface = it }
            },
            update = { v ->
                v.eyedropper = tool == Tool.Eyedropper
                v.pickWash = pickKind == PickKind.Wash
                v.onSample = { setColor(it) }
                v.onTouchStart = { panel = Panel.None }
                v.onPenEraser = { on ->
                    if (on != penEraser) {
                        penEraser = on
                        // Now, so the stroke that is starting already uses it.
                        applyBrush()
                    }
                }
                v.setCanvasSize(info.width, info.height, info.home)
            },
        )

        AnimatedVisibility(!hidden, enter = fadeIn(), exit = fadeOut()) {
        Box(Modifier.fillMaxSize().graphicsLayer { alpha = chrome }) {
            // Top bar. Actions that don't fit the screen's width go in the ⋮ menu.
            BoxWithConstraints(Modifier.fillMaxWidth().statusBarsPadding()) {
                val btn = if (maxWidth < 600.dp) 40.dp else 44.dp
                val actions = listOf(
                    iconAction(
                        if (dip) CupIcon else Icons.Outlined.AllInclusive,
                        if (dip) "Brush runs dry: dip it in the cup" else "Brush always wet",
                        selected = dip,
                    ) {
                        dip = !dip
                        app.settings.dip = dip
                        Toast.makeText(context, if (dip) "Brush runs dry: dip it in the cup" else "Brush always wet", Toast.LENGTH_SHORT).show()
                    },
                    BarAction(
                        "Tip the paper",
                        selected = gimbal || keepTilt,
                        menuIcon = { GimbalGlyph(tilt, if (gimbal || keepTilt || tilt != Offset.Zero) AquaColors.Accent else AquaColors.Text) },
                        button = { s -> GimbalButton(tilt, selected = gimbal || keepTilt, size = s) { panel = Panel.None; gimbal = !gimbal } },
                        onClick = { panel = Panel.None; gimbal = !gimbal },
                    ),
                    iconAction(Icons.Outlined.Sync, if (syncOn) "Sync: on" else "Sync", tint = syncTint(syncOn, syncStatus.primary, syncStatus.backup)) {
                        panel = Panel.None
                        syncDialog = true
                    },
                    iconAction(Icons.Outlined.IosShare, "Export") {
                        scope.launch {
                            val ok = exportToGallery(context, engine)
                            Toast.makeText(context, if (ok) "Saved to Pictures/Aqua" else "Export failed", Toast.LENGTH_SHORT).show()
                        }
                    },
                    iconAction(Icons.Outlined.VisibilityOff, "Hide controls") {
                        panel = Panel.None
                        gimbal = false
                        hidden = true
                    },
                )
                // Back, undo, redo; dry, layers, paint and ⋮.
                val slots = barSlots(maxWidth, btn, fixedButtons = 7)
                val inline = if (slots >= actions.size) actions else actions.take(slots)
                val overflow = actions.drop(inline.size)
                Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Bar {
                        RoundButton(Icons.AutoMirrored.Outlined.ArrowBack, "Gallery", size = btn) { leave() }
                        RoundButton(Icons.AutoMirrored.Outlined.Undo, "Undo", enabled = info.canUndo, size = btn) { Native.undo(engine); refresh() }
                        RoundButton(Icons.AutoMirrored.Outlined.Redo, "Redo", enabled = info.canRedo, size = btn) { Native.redo(engine); refresh() }
                    }
                    Spacer(Modifier.weight(1f))
                    Bar {
                        val activeWet = info.layers.getOrNull(info.active)?.wet == true
                        AnimatedVisibility(activeWet, enter = fadeIn(), exit = fadeOut()) {
                            RoundButton(Icons.Outlined.Air, "Dry layer now", size = btn) {
                                Native.layerOp(engine, LayerOp.DRY, info.active, 0f, null)
                                refresh()
                            }
                        }
                        inline.forEach { it.button(btn) }
                        RoundButton(Icons.Outlined.Layers, "Layers", selected = panel == Panel.Layers, size = btn) {
                            panel = if (panel == Panel.Layers) Panel.None else Panel.Layers
                        }
                        PigmentSwatch(color, Modifier.padding(horizontal = (btn - 32.dp) / 2), size = 32.dp, selected = panel == Panel.Palette) {
                            panel = if (panel == Panel.Palette) Panel.None else Panel.Palette
                        }
                        if (overflow.isNotEmpty()) MoreMenu(overflow, btn)
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
                    Tool.Eraser, Tool.Push -> VerticalRailSlider(bp.strength, { params[brushTool] = bp.copy(strength = it) }, height = 120.dp, label = "strength")
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
                if (!dip) RoundButton(Icons.Outlined.WaterDrop, "Clear water", selected = tool == Tool.Water, size = 48.dp) { selectTool(Tool.Water) }
                RoundButton(Icons.Outlined.HistoryEdu, "Ink", selected = tool == Tool.Ink, size = 48.dp) { selectTool(Tool.Ink) }
                RoundButton(EraserIcon, "Eraser", selected = tool == Tool.Eraser || penEraser, size = 48.dp) { selectTool(Tool.Eraser) }
                if (!dip) RoundButton(PushIcon, "Push", selected = tool == Tool.Push, size = 48.dp) { selectTool(Tool.Push) }
                RoundButton(if (pickKind == PickKind.Wash) WashPickIcon else Icons.Outlined.Colorize, "Eyedropper", selected = tool == Tool.Eyedropper, size = 48.dp) { selectTool(Tool.Eyedropper) }
            }

            AnimatedVisibility(
                dip,
                enter = fadeIn(),
                exit = fadeOut(),
                modifier = Modifier.align(Alignment.BottomStart).navigationBarsPadding().padding(start = 12.dp, bottom = 12.dp),
            ) {
                // With the pen it is an ink well, of the pen's ink.
                if (brushTool == Tool.Ink) {
                    WaterCup(info.inkCharge, { dipBrush() }, ink = Color(ink.masstoneArgb))
                } else {
                    WaterCup(info.charge ?: 1f, { dipBrush() })
                }
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
                Panel.Picker -> PickerPanel(
                    pickKind, { pickKind = it; app.settings.pickKind = it; panel = Panel.None },
                    Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 80.dp),
                )
                Panel.None -> {}
            }
        }
        }

        // With the controls hidden, the one way back to them.
        AnimatedVisibility(
            hidden && !exiting,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(8.dp),
        ) {
            Box(Modifier.clip(RoundedCornerShape(24.dp)).background(AquaColors.Panel.copy(alpha = 0.5f))) {
                RoundButton(Icons.Outlined.Visibility, "Show controls", tint = AquaColors.Text.copy(alpha = 0.7f)) { hidden = false }
            }
        }

        if (syncDialog) {
            SyncDialog(
                app.settings, app.sync,
                onToggle = { syncOn = it },
                onSyncNow = { scope.launch(Dispatchers.IO) { save(force = true) } },
                onDismiss = { syncDialog = false },
            )
        }

        AnimatedVisibility(gimbal && !exiting, enter = fadeIn(), exit = fadeOut()) {
            GimbalOverlay(
                tilt, { tilt = it }, gyro,
                canKeepOn = canKeepTilt, keepOn = keepTilt, onKeepOn = { keepTilt = it },
                onDismiss = { gimbal = false },
            )
        }
        // Not while the app is in the background.
        val resumed by lifecycle.currentStateAsState()
        PhoneTilt((gimbal || keepTilt) && resumed.isAtLeast(Lifecycle.State.RESUMED) && !exiting, gyro, tilt) { tilt = it }

        AnimatedVisibility(
            overviewHint && !exiting,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 36.dp),
        ) {
            Text(
                if (pinchToGallery) "Release for gallery" else "Release to fit",
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
