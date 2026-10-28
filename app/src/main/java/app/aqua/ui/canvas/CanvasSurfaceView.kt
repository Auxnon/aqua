package app.aqua.ui.canvas

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Matrix
import android.hardware.display.DisplayManager
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewConfiguration
import app.aqua.engine.Native
import app.aqua.ui.CanvasFrame
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/** Pinching out below this fraction of the fitted size and letting go opens the gallery. */
private const val OVERVIEW_ZOOM = 0.45f

/**
 * Hosts the engine's swapchain and turns touches into strokes and view gestures:
 * one finger / stylus paints, two fingers pan-zoom-rotate, a two-finger tap undoes
 * and a three-finger tap redoes. Once a stylus has been seen, fingers only navigate.
 * Pinching far out past the fitted view and letting go goes back to the gallery.
 */
@SuppressLint("ViewConstructor")
class CanvasSurfaceView(context: Context, private val engine: Long) : SurfaceView(context), SurfaceHolder.Callback {
    var onStrokeEnd: () -> Unit = {}
    var onUndo: () -> Unit = {}
    var onRedo: () -> Unit = {}
    var onSample: (FloatArray) -> Unit = {}
    var onTouchStart: () -> Unit = {}
    /** True while a pinch is far enough out that letting go would leave for the gallery. */
    var onOverviewHint: (Boolean) -> Unit = {}
    var onOverview: (CanvasFrame) -> Unit = {}
    /** The screen-space tilt after the device turned, so the paper stays tipped the same way. */
    var onTiltTurned: (Float, Float) -> Unit = { _, _ -> }
    var eyedropper = false

    private var canvasW = 0
    private var canvasH = 0
    private var attached = false
    private var fitted = false
    private var fitScale = 1f
    private var overviewHint = false
    private var tiltX = 0f
    private var tiltY = 0f
    private var rotation = -1
    private var viewW = 0
    private var viewH = 0
    // How far the view has been turned back against the device turning, in degrees.
    private var turn = 0f
    private val view = Matrix()
    private val inverse = Matrix()
    private val tmp = FloatArray(2)

    private enum class Mode { None, Drawing, Gesture, Eyedrop }
    private var mode = Mode.None
    private var drawPointer = -1
    private var drawWithStylus = false
    private var stylusSeen = false
    private var downTime = 0L
    private var maxPointers = 0
    private var moved = false
    private val slop = ViewConfiguration.get(context).scaledTouchSlop.toFloat()

    private val gestureStart = Matrix()
    private var g0x = 0f
    private var g0y = 0f
    private var gDist = 1f
    private var gAngle = 0f
    private var gPointers = 0
    private var lastX = 0f
    private var lastY = 0f

    // A half turn keeps the window's size, so only the display says it happened.
    private val displays = context.getSystemService(DisplayManager::class.java)
    private val displayListener = object : DisplayManager.DisplayListener {
        override fun onDisplayChanged(id: Int) {
            val r = display?.rotation ?: return
            if (rotation >= 0 && (r - rotation + 4) % 4 == 2) {
                followDisplay(viewW, viewH)
                pushView()
            }
        }

        override fun onDisplayAdded(id: Int) {}
        override fun onDisplayRemoved(id: Int) {}
    }

    init {
        holder.addCallback(this)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        displays.registerDisplayListener(displayListener, handler)
    }

    override fun onDetachedFromWindow() {
        displays.unregisterDisplayListener(displayListener)
        super.onDetachedFromWindow()
    }

    /**
     * Keeps the paper still in the hand when the device turns: the window turns
     * under it, so the view turns the other way and only the controls move.
     */
    private fun followDisplay(w: Int, h: Int) {
        val r = display?.rotation ?: return
        val steps = if (rotation < 0) 0 else (r - rotation + 4) % 4
        if (steps != 0 && viewW > 0) {
            val deg = -90f * steps
            view.postTranslate(-viewW / 2f, -viewH / 2f)
            view.postRotate(deg)
            view.postTranslate(w / 2f, h / 2f)
            turn = ((turn + deg) % 360f + 540f) % 360f - 180f
            // The tilt is in screen space too; turn it with the paper.
            if (tiltX != 0f || tiltY != 0f) {
                val a = Math.toRadians(deg.toDouble())
                val c = cos(a).toFloat()
                val s = sin(a).toFloat()
                val x = tiltX * c - tiltY * s
                tiltY = tiltX * s + tiltY * c
                tiltX = x
                onTiltTurned(tiltX, tiltY)
            }
        }
        rotation = r
        viewW = w
        viewH = h
    }

    fun setCanvasSize(w: Int, h: Int) {
        if (w != canvasW || h != canvasH) {
            canvasW = w
            canvasH = h
            fitted = false
        }
        fitIfReady()
    }

    fun fit() {
        if (width == 0 || canvasW == 0) return
        val s = min(width * 0.92f / canvasW, height * 0.84f / canvasH)
        fitScale = s
        turn = 0f
        view.setScale(s, s)
        view.postTranslate((width - canvasW * s) / 2f, (height - canvasH * s) / 2f)
        pushView()
    }

    private fun fitIfReady() {
        if (!fitted && width > 0 && canvasW > 0) {
            fitted = true
            fit()
        }
    }

    private fun scale(): Float {
        val m = FloatArray(9).also { view.getValues(it) }
        return sqrt(abs(m[0] * m[4] - m[1] * m[3]))
    }

    private fun pushView() {
        view.invert(inverse)
        val v = FloatArray(9)
        inverse.getValues(v)
        Native.setView(engine, floatArrayOf(v[0], v[1], v[2], v[3], v[4], v[5], scale()))
        if (tiltX != 0f || tiltY != 0f) pushTilt()
    }

    /** Tips the paper towards screen direction (x, y), length 0 (level) to 1 (steep). */
    fun setTilt(x: Float, y: Float) {
        tiltX = x
        tiltY = y
        pushTilt()
    }

    /** The paper turns with the view, so the downhill direction is turned back into canvas space. */
    private fun pushTilt() {
        val m = FloatArray(9).also { view.getValues(it) }
        val a = -atan2(m[Matrix.MSKEW_Y], m[Matrix.MSCALE_X])
        Native.setTilt(engine, tiltX * cos(a) - tiltY * sin(a), tiltX * sin(a) + tiltY * cos(a))
    }

    private fun pastOverview() = canvasW > 0 && scale() < fitScale * OVERVIEW_ZOOM

    private fun setOverviewHint(on: Boolean) {
        if (on != overviewHint) {
            overviewHint = on
            onOverviewHint(on)
        }
    }

    /** Where the canvas is on screen right now (this view fills the window). */
    fun frame(): CanvasFrame? {
        if (canvasW == 0) return null
        val m = FloatArray(9).also { view.getValues(it) }
        val c = floatArrayOf(canvasW / 2f, canvasH / 2f)
        view.mapPoints(c)
        val s = scale()
        val rot = Math.toDegrees(atan2(m[Matrix.MSKEW_Y], m[Matrix.MSCALE_X]).toDouble()).toFloat()
        return CanvasFrame(c[0], c[1], canvasW * s, canvasH * s, rot, turn)
    }

    override fun surfaceCreated(holder: SurfaceHolder) {}

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, w: Int, h: Int) {
        if (!attached) {
            Native.setSurface(engine, holder.surface, w, h)
            attached = true
        } else {
            Native.resize(engine, w, h)
        }
        followDisplay(w, h)
        fitIfReady()
        pushView()
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        // Blocks until the engine has let go of the window, as Android requires.
        Native.setSurface(engine, null, 0, 0)
        attached = false
    }

    private fun toCanvas(x: Float, y: Float): FloatArray {
        tmp[0] = x
        tmp[1] = y
        inverse.mapPoints(tmp)
        return tmp
    }

    private fun pressure(e: MotionEvent, i: Int, h: Int = -1): Float {
        if (e.getToolType(i) == MotionEvent.TOOL_TYPE_FINGER) return 0.8f
        val p = if (h < 0) e.getPressure(i) else e.getHistoricalPressure(i, h)
        return p.coerceIn(0.02f, 1f)
    }

    private fun sendPoints(e: MotionEvent, phase: Int) {
        val i = e.findPointerIndex(drawPointer)
        if (i < 0) return
        val n = if (phase == 1) e.historySize else 0
        val out = FloatArray((n + 1) * 3)
        for (h in 0 until n) {
            val c = toCanvas(e.getHistoricalX(i, h), e.getHistoricalY(i, h))
            out[h * 3] = c[0]
            out[h * 3 + 1] = c[1]
            out[h * 3 + 2] = pressure(e, i, h)
        }
        val c = toCanvas(e.getX(i), e.getY(i))
        out[n * 3] = c[0]
        out[n * 3 + 1] = c[1]
        out[n * 3 + 2] = pressure(e, i)
        Native.stroke(engine, phase, out)
    }

    private fun sample(x: Float, y: Float) {
        val c = toCanvas(x, y)
        Native.sample(engine, c[0], c[1])?.let(onSample)
    }

    private fun beginGesture(e: MotionEvent) {
        gestureStart.set(view)
        gPointers = min(e.pointerCount, 2)
        if (gPointers >= 2) {
            val (x0, y0, x1, y1) = listOf(e.getX(0), e.getY(0), e.getX(1), e.getY(1))
            g0x = (x0 + x1) / 2f
            g0y = (y0 + y1) / 2f
            gDist = hypot(x1 - x0, y1 - y0).coerceAtLeast(1f)
            gAngle = atan2(y1 - y0, x1 - x0)
        } else {
            g0x = e.getX(0)
            g0y = e.getY(0)
        }
    }

    private fun updateGesture(e: MotionEvent) {
        if (e.pointerCount < gPointers) return
        view.set(gestureStart)
        if (gPointers >= 2) {
            val (x0, y0, x1, y1) = listOf(e.getX(0), e.getY(0), e.getX(1), e.getY(1))
            val cx = (x0 + x1) / 2f
            val cy = (y0 + y1) / 2f
            var s = hypot(x1 - x0, y1 - y0).coerceAtLeast(1f) / gDist
            val m = FloatArray(9).also { gestureStart.getValues(it) }
            val base = sqrt(abs(m[0] * m[4] - m[1] * m[3]))
            s = (base * s).coerceIn(0.05f, 32f) / base
            var deg = Math.toDegrees((atan2(y1 - y0, x1 - x0) - gAngle).toDouble()).toFloat()
            if (abs(deg) < 4f) deg = 0f
            view.postTranslate(-g0x, -g0y)
            view.postScale(s, s)
            view.postRotate(deg)
            view.postTranslate(cx, cy)
            if (hypot(cx - g0x, cy - g0y) > slop || abs(s - 1f) > 0.04f || deg != 0f) moved = true
            setOverviewHint(pastOverview())
        } else {
            val dx = e.getX(0) - g0x
            val dy = e.getY(0) - g0y
            view.postTranslate(dx, dy)
            if (hypot(dx, dy) > slop) moved = true
        }
        pushView()
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                requestUnbufferedDispatch(e)
                onTouchStart()
                val tool = e.getToolType(0)
                val isStylus = tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER
                if (isStylus) stylusSeen = true
                downTime = e.eventTime
                maxPointers = 1
                moved = false
                lastX = e.x
                lastY = e.y
                when {
                    !isStylus && stylusSeen -> {
                        mode = Mode.Gesture
                        beginGesture(e)
                    }
                    eyedropper -> {
                        mode = Mode.Eyedrop
                        sample(e.x, e.y)
                    }
                    else -> {
                        mode = Mode.Drawing
                        drawPointer = e.getPointerId(0)
                        drawWithStylus = isStylus
                        sendPoints(e, 0)
                    }
                }
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                maxPointers = maxOf(maxPointers, e.pointerCount)
                val newIsFinger = e.getToolType(e.actionIndex) == MotionEvent.TOOL_TYPE_FINGER
                when (mode) {
                    Mode.Drawing -> if (!(drawWithStylus && newIsFinger)) {
                        // A second finger right after the first means this was a gesture all along.
                        val quick = e.eventTime - downTime < 250
                        Native.stroke(engine, if (quick) 3 else 2, FloatArray(0))
                        mode = Mode.Gesture
                        moved = !quick
                        beginGesture(e)
                    }
                    Mode.Gesture, Mode.Eyedrop -> {
                        mode = Mode.Gesture
                        beginGesture(e)
                    }
                    Mode.None -> {}
                }
            }
            MotionEvent.ACTION_MOVE -> when (mode) {
                Mode.Drawing -> sendPoints(e, 1)
                Mode.Gesture -> updateGesture(e)
                Mode.Eyedrop -> sample(e.x, e.y)
                Mode.None -> {}
            }
            MotionEvent.ACTION_POINTER_UP -> {
                if (mode == Mode.Drawing && e.getPointerId(e.actionIndex) == drawPointer) {
                    sendPoints(e, 2)
                    mode = Mode.None
                    onStrokeEnd()
                } else if (mode == Mode.Gesture) {
                    // Re-anchor on the pointers that remain, so lifting one finger doesn't jump.
                    val remaining = (0 until e.pointerCount).filter { it != e.actionIndex }
                    if (remaining.isNotEmpty()) {
                        gestureStart.set(view)
                        if (remaining.size >= 2) {
                            val a = remaining[0]
                            val b = remaining[1]
                            g0x = (e.getX(a) + e.getX(b)) / 2f
                            g0y = (e.getY(a) + e.getY(b)) / 2f
                            gDist = hypot(e.getX(b) - e.getX(a), e.getY(b) - e.getY(a)).coerceAtLeast(1f)
                            gAngle = atan2(e.getY(b) - e.getY(a), e.getX(b) - e.getX(a))
                            gPointers = 2
                        } else {
                            g0x = e.getX(remaining[0])
                            g0y = e.getY(remaining[0])
                            gPointers = 1
                        }
                    }
                }
            }
            MotionEvent.ACTION_UP -> {
                when (mode) {
                    Mode.Drawing -> {
                        sendPoints(e, 2)
                        onStrokeEnd()
                    }
                    Mode.Gesture -> if (!moved && e.eventTime - downTime < 350) {
                        when (maxPointers) {
                            2 -> onUndo()
                            3 -> onRedo()
                        }
                    } else if (overviewHint && pastOverview()) {
                        frame()?.let(onOverview)
                    }
                    else -> {}
                }
                mode = Mode.None
            }
            MotionEvent.ACTION_CANCEL -> {
                if (mode == Mode.Drawing) Native.stroke(engine, 3, FloatArray(0))
                setOverviewHint(false)
                mode = Mode.None
            }
        }
        return true
    }
}
