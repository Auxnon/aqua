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

    /** The brush runs out of water and has to be dipped in the cup (off: it is always wet). */
    var dip: Boolean
        get() = prefs.getBoolean("dip", false)
        set(v) = prefs.edit().putBoolean("dip", v).apply()

    /** Push each save to the sync daemon(s). */
    var syncOn: Boolean
        get() = prefs.getBoolean("sync_on", false)
        set(v) = prefs.edit().putBoolean("sync_on", v).apply()

    /** The daemon tried first, as "192.168.1.20" or "host:port". */
    var syncPrimary: String
        get() = prefs.getString("sync_primary", "") ?: ""
        set(v) = prefs.edit().putString("sync_primary", v.trim()).apply()

    /** A second daemon that gets a backup copy a few seconds later. */
    var syncBackup: String
        get() = prefs.getString("sync_backup", "") ?: ""
        set(v) = prefs.edit().putString("sync_backup", v.trim()).apply()

    /** The daemons' key, if they were started with one. */
    var syncKey: String
        get() = prefs.getString("sync_key", "") ?: ""
        set(v) = prefs.edit().putString("sync_key", v.trim()).apply()
}
