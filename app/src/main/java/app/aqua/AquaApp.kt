package app.aqua

import android.app.Application
import app.aqua.data.ArtRepository
import app.aqua.engine.Native
import java.io.File

class AquaApp : Application() {
    /** The engine lives for the whole process; its GPU thread idles when nothing is happening. */
    val engine: Long by lazy { Native.create() }

    /** The shared mixing palette. */
    val palette: Long by lazy {
        Native.paletteCreate(PALETTE_SIZE, PALETTE_SIZE).also { Native.paletteLoad(it, paletteFile.path) }
    }

    val paletteFile: File get() = File(filesDir, "palette.bin")

    val repo: ArtRepository by lazy { ArtRepository(File(filesDir, "artworks")) }

    fun savePalette() {
        Native.paletteSave(palette, paletteFile.path)
    }

    companion object {
        const val PALETTE_SIZE = 256
    }
}
