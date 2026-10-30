package app.aqua.engine

import android.view.Surface

/** JNI surface of the Rust engine (engine/src/android.rs). */
object Native {
    init {
        System.loadLibrary("aqua_engine")
    }

    @JvmStatic external fun create(): Long
    @JvmStatic external fun destroy(handle: Long)
    @JvmStatic external fun setSurface(handle: Long, surface: Surface?, width: Int, height: Int)
    @JvmStatic external fun resize(handle: Long, width: Int, height: Int)

    /** canvasFromScreen affine (a, b, c, d, e, f) followed by the zoom factor. */
    @JvmStatic external fun setView(handle: Long, view: FloatArray)

    /** Tips the paper: (x, y) is the downhill direction in canvas pixels' axes, length 0 (level) to 1 (steep). */
    @JvmStatic external fun setTilt(handle: Long, x: Float, y: Float)

    /** See [BrushParams.toArray] for the layout. */
    @JvmStatic external fun setBrush(handle: Long, tool: Int, params: FloatArray)

    /** phase: 0 begin, 1 move, 2 end, 3 cancel; points are (x, y, pressure) in canvas pixels. */
    @JvmStatic external fun stroke(handle: Long, phase: Int, points: FloatArray)

    @JvmStatic external fun newCanvas(handle: Long, width: Int, height: Int, paper: Int, seed: Int, home: Int)
    @JvmStatic external fun load(handle: Long, dir: String): Boolean
    @JvmStatic external fun save(handle: Long, dir: String): Boolean
    @JvmStatic external fun exportPng(handle: Long, path: String): Boolean
    @JvmStatic external fun closeCanvas(handle: Long)

    /** op: 0 add, 1 remove, 2 move to value, 3 select, 4 opacity, 5 visible, 6 rename, 7 dry, 8 clear, 9 drying speed */
    @JvmStatic external fun layerOp(handle: Long, op: Int, index: Int, value: Float, name: String?)
    @JvmStatic external fun undo(handle: Long)
    @JvmStatic external fun redo(handle: Long)
    @JvmStatic external fun query(handle: Long): String
    @JvmStatic external fun sample(handle: Long, x: Float, y: Float): FloatArray?
    @JvmStatic external fun samplePigment(handle: Long, x: Float, y: Float): FloatArray?

    @JvmStatic external fun pigmentFromRgb(r: Float, g: Float, b: Float): FloatArray
    @JvmStatic external fun pigmentToRgb(pigment: FloatArray): FloatArray

    @JvmStatic external fun paletteCreate(width: Int, height: Int): Long
    @JvmStatic external fun paletteDestroy(palette: Long)
    @JvmStatic external fun paletteDab(palette: Long, x: Float, y: Float, radius: Float, pigment: FloatArray, amount: Float)
    @JvmStatic external fun paletteSmudgeBegin(palette: Long, x: Float, y: Float, radius: Float)
    @JvmStatic external fun paletteSmudge(palette: Long, x0: Float, y0: Float, x1: Float, y1: Float, strength: Float)
    @JvmStatic external fun paletteSmudgeEnd(palette: Long)
    @JvmStatic external fun palettePick(palette: Long, x: Float, y: Float, radius: Float): FloatArray?
    @JvmStatic external fun paletteRender(palette: Long, out: IntArray)
    @JvmStatic external fun paletteClear(palette: Long)
    @JvmStatic external fun paletteSave(palette: Long, path: String): Boolean
    @JvmStatic external fun paletteLoad(palette: Long, path: String): Boolean
}

object LayerOp {
    const val ADD = 0
    const val REMOVE = 1
    const val MOVE = 2
    const val SELECT = 3
    const val OPACITY = 4
    const val VISIBLE = 5
    const val RENAME = 6
    const val DRY = 7
    const val CLEAR = 8
    const val DRYING_SPEED = 9
    const val FLOW = 10
}
