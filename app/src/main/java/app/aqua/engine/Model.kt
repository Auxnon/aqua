package app.aqua.engine

import org.json.JSONObject

enum class Tool(val id: Int) { Watercolor(0), Water(1), Ink(2), Eraser(3), Push(4), Eyedropper(-1) }

/** A pigment as Kubelka-Munk absorption (rgb) and scattering, at unit concentration. */
data class Pigment(val k: FloatArray) {
    /** How it looks on white paper; thinned paint shows as its lighter tint. */
    val argb: Int by lazy { toArgb(k) }

    /** How concentrated the paint is: 1 = full strength, less = thinned with water. */
    val strength: Float get() = (k[3] / UNIT_S).coerceIn(0f, 1f)

    /** The colour at full strength. */
    val masstoneArgb: Int by lazy { if (strength >= 1f) argb else toArgb(FloatArray(4) { k[it] / strength.coerceAtLeast(1e-4f) }) }

    override fun equals(other: Any?) = other is Pigment && other.k.contentEquals(k)
    override fun hashCode() = k.contentHashCode()

    companion object {
        /** Scattering of one unit of pigment (`pigment::UNIT_S` in the engine). */
        const val UNIT_S = 0.05f

        private fun toArgb(k: FloatArray): Int {
            val c = Native.pigmentToRgb(k)
            return android.graphics.Color.argb(1f, c[0], c[1], c[2])
        }

        fun fromRgb(r: Float, g: Float, b: Float) = Pigment(Native.pigmentFromRgb(r, g, b))
        fun fromArgb(c: Int) = fromRgb(
            android.graphics.Color.red(c) / 255f,
            android.graphics.Color.green(c) / 255f,
            android.graphics.Color.blue(c) / 255f,
        )
    }
}

data class BrushParams(
    val size: Float,
    val load: Float = 0.6f,
    val water: Float = 0.6f,
    val softness: Float = 0.18f,
    val bristle: Float = 0.5f,
    val bleed: Float = 0f,
    val strength: Float = 0.8f,
    val pressureSize: Float = 0.6f,
    val pressureFlow: Float = 0.5f,
    val smoothing: Float = 0.3f,
    val splatter: Float = 0f,
) {
    fun toArray(p: Pigment) = floatArrayOf(
        size, p.k[0], p.k[1], p.k[2], p.k[3], load, water, softness, bristle, bleed, strength,
        pressureSize, pressureFlow, smoothing, splatter,
    )
}

data class LayerInfo(
    val id: Int,
    val name: String,
    val opacity: Float,
    val visible: Boolean,
    val wet: Boolean,
    val wetness: Float,
    val drying: Float,
    val flow: Float,
)

data class CanvasInfo(
    val width: Int = 0,
    val height: Int = 0,
    /** Which way up the painting is: the screen's quarter turns when it was created. */
    val home: Int = 0,
    val active: Int = 0,
    val canUndo: Boolean = false,
    val canRedo: Boolean = false,
    val revision: Long = 0,
    val layers: List<LayerInfo> = emptyList(),
) {
    companion object {
        fun parse(json: String): CanvasInfo {
            val o = runCatching { JSONObject(json) }.getOrNull() ?: return CanvasInfo()
            if (!o.has("layers")) return CanvasInfo()
            val arr = o.getJSONArray("layers")
            val layers = (0 until arr.length()).map {
                val l = arr.getJSONObject(it)
                LayerInfo(
                    id = l.getInt("id"),
                    name = l.getString("name"),
                    opacity = l.getDouble("opacity").toFloat(),
                    visible = l.getBoolean("visible"),
                    wet = l.getBoolean("wet"),
                    wetness = l.getDouble("wetness").toFloat(),
                    drying = l.getDouble("drying").toFloat(),
                    flow = l.optDouble("flow", 0.5).toFloat(),
                )
            }
            return CanvasInfo(
                width = o.getInt("width"),
                height = o.getInt("height"),
                home = o.optInt("home", 0),
                active = o.getInt("active"),
                canUndo = o.getBoolean("canUndo"),
                canRedo = o.getBoolean("canRedo"),
                revision = o.getLong("revision"),
                layers = layers,
            )
        }
    }
}
