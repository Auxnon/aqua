package app.aqua.data

import android.content.Context

/** What pinching far out past the fitted view does. */
enum class PinchOut { FitView, Gallery }

/** Small app preferences, kept across sessions. */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var pinchOut: PinchOut
        get() = if (prefs.getString("pinch_out", null) == "gallery") PinchOut.Gallery else PinchOut.FitView
        set(v) = prefs.edit().putString("pinch_out", if (v == PinchOut.Gallery) "gallery" else "fit").apply()
}
