package app.aqua.data

import org.json.JSONObject
import java.io.File
import java.util.UUID

/** A saved painting; [home] is which way up it is (the screen's quarter turns when it was created). */
data class Artwork(val dir: File, val width: Int, val height: Int, val modified: Long, val home: Int = 0) {
    val id: String get() = dir.name
    val thumb: File get() = File(dir, "thumb.png")
}

/** Paintings are directories under [root]; the engine reads and writes their contents. */
class ArtRepository(private val root: File) {
    init {
        root.mkdirs()
    }

    fun list(): List<Artwork> = (root.listFiles() ?: emptyArray())
        .filter { File(it, "meta.json").exists() }
        .mapNotNull { dir ->
            runCatching {
                val meta = File(dir, "meta.json")
                val o = JSONObject(meta.readText())
                Artwork(dir, o.getInt("width"), o.getInt("height"), meta.lastModified(), o.optInt("home", 0) % 4)
            }.getOrNull()
        }
        .sortedByDescending { it.modified }

    fun newDir(): File = File(root, UUID.randomUUID().toString()).also { it.mkdirs() }

    fun delete(art: Artwork) {
        art.dir.deleteRecursively()
    }

    fun duplicate(art: Artwork): File {
        val dst = newDir()
        art.dir.copyRecursively(dst, overwrite = true)
        File(dst, "meta.json").setLastModified(System.currentTimeMillis())
        return dst
    }

    /** Removes directories left behind by a canvas that was never saved. */
    fun prune() {
        root.listFiles()?.filter { !File(it, "meta.json").exists() }?.forEach { it.deleteRecursively() }
    }
}
