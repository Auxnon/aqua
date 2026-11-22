package app.aqua.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** How the last push to one daemon went. */
sealed interface PushState {
    data object Idle : PushState
    data object Sending : PushState
    data class Done(val at: Long) : PushState
    data class Failed(val message: String) : PushState
}

data class SyncStatus(val primary: PushState = PushState.Idle, val backup: PushState = PushState.Idle)

/**
 * Pushes saved paintings to aqua-sync daemons (see sync/src/lib.rs for the
 * protocol): the primary at once, the backup [BACKUP_DELAY_MS] later. A newer
 * save before then replaces the backup's copy, so it gets the latest.
 */
class Sync(private val settings: Settings) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val _status = MutableStateFlow(SyncStatus())
    val status: StateFlow<SyncStatus> = _status
    private var backupJob: Job? = null

    /** Sends the painting saved in [dir], if sync is on (or [force]d). */
    fun push(dir: File, force: Boolean = false) {
        if (!settings.syncOn && !force) return
        val primary = settings.syncPrimary
        val backup = settings.syncBackup
        if (primary.isEmpty() && backup.isEmpty()) return
        scope.launch {
            // Read once, now: the files can change under a later save.
            val body = runCatching { encode(dir) }.getOrElse {
                _status.update { it.copy(primary = PushState.Failed("couldn't read the painting")) }
                return@launch
            }
            val key = settings.syncKey
            if (backup.isNotEmpty()) {
                backupJob?.cancel()
                backupJob = scope.launch {
                    delay(BACKUP_DELAY_MS)
                    send(backup, key, dir.name, body) { s -> _status.update { it.copy(backup = s) } }
                }
            }
            if (primary.isNotEmpty()) send(primary, key, dir.name, body) { s -> _status.update { it.copy(primary = s) } }
        }
    }

    private fun send(addr: String, key: String, id: String, body: ByteArray, set: (PushState) -> Unit) {
        set(PushState.Sending)
        set(
            runCatching { post(addr, key, id, body) }.fold(
                { PushState.Done(System.currentTimeMillis()) },
                { PushState.Failed(it.message ?: it.javaClass.simpleName) },
            ),
        )
    }

    companion object {
        const val PORT = 7717
        const val BACKUP_DELAY_MS = 5_000L

        fun url(addr: String, path: String): URL {
            val a = addr.trim().removePrefix("http://").trimEnd('/')
            val hasPort = a.substringAfterLast(':', "").toIntOrNull() != null && !a.endsWith("]")
            return URL("http://${if (hasPort) a else "$a:$PORT"}$path")
        }

        /** "AQS1", then per file: u16 name length, name, u32 data length, data (little endian). */
        fun encode(dir: File): ByteArray {
            val files = (dir.listFiles() ?: emptyArray())
                .filter { it.isFile && !it.name.startsWith(".") && !it.name.endsWith(".tmp") }
                // meta.json last, as when saving.
                .sortedBy { it.name == "meta.json" }
            require(files.any { it.name == "meta.json" })
            val out = ByteArrayOutputStream(files.sumOf { it.length() + 6 }.toInt() + 4)
            out.write("AQS1".toByteArray())
            for (f in files) {
                val name = f.name.toByteArray()
                val data = f.readBytes()
                out.write(ByteBuffer.allocate(2).order(ByteOrder.LITTLE_ENDIAN).putShort(name.size.toShort()).array())
                out.write(name)
                out.write(ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(data.size).array())
                out.write(data)
            }
            return out.toByteArray()
        }

        fun post(addr: String, key: String, id: String, body: ByteArray) {
            val c = url(addr, "/v1/paintings/$id").openConnection() as HttpURLConnection
            try {
                c.requestMethod = "POST"
                c.connectTimeout = 4_000
                c.readTimeout = 60_000
                c.doOutput = true
                c.setFixedLengthStreamingMode(body.size)
                c.setRequestProperty("Content-Type", "application/octet-stream")
                c.setRequestProperty("X-Aqua-Key", key)
                c.outputStream.use { it.write(body) }
                when (val code = c.responseCode) {
                    200 -> {}
                    401 -> error("wrong sync key")
                    else -> error("refused ($code)")
                }
            } finally {
                c.disconnect()
            }
        }

        /** Checks that a daemon answers at [addr]; returns its greeting. */
        fun hello(addr: String): String {
            val c = url(addr, "/v1/hello").openConnection() as HttpURLConnection
            try {
                c.connectTimeout = 3_000
                c.readTimeout = 5_000
                if (c.responseCode != 200) error("not an aqua-sync daemon")
                val text = c.inputStream.bufferedReader().readText()
                return Regex("\"message\":\"([^\"]*)\"").find(text)?.groupValues?.get(1) ?: "aqua-sync"
            } finally {
                c.disconnect()
            }
        }
    }
}
