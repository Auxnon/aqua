package app.aqua.data

import android.content.Context

/** What pinching far out past the fitted view does. */
enum class PinchOut { FitView, Gallery }

/**
 * How the eyedropper reads the canvas: as a wash (whiter means thinner paint of a
 * deeper pigment) or as the exact colour at full strength.
 */
enum class PickKind { Wash, Classic }

/** Small app preferences, kept across sessions. */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var pinchOut: PinchOut
        get() = if (prefs.getString("pinch_out", null) == "gallery") PinchOut.Gallery else PinchOut.FitView
        set(v) = prefs.edit().putString("pinch_out", if (v == PinchOut.Gallery) "gallery" else "fit").apply()

    var pickKind: PickKind
        get() = if (prefs.getString("pick_kind", null) == "classic") PickKind.Classic else PickKind.Wash
        set(v) = prefs.edit().putString("pick_kind", if (v == PickKind.Classic) "classic" else "wash").apply()
}
