package dev.bobix.phone.push

import java.io.File
import java.io.RandomAccessFile

/**
 * What the push service knows about each paired computer: its `box` public key and the receive window of its `event`
 * stream. The app writes the key when it pairs; the service moves the window when it accepts a notice.
 *
 * One file per computer (`<id>.peer`: the key in hex on the first line, the window JSON after it). Reading the window,
 * opening a packet and storing the new window must happen under [locked]: a push can arrive while the app is writing, and
 * two pushes can arrive together, so without the lock two notices could use one counter or one could erase the other's.
 */
class PeerStore(private val dir: File) {
    class Peer(val id: String, val boxPublic: ByteArray, val state: String)

    fun <T> locked(block: () -> T): T = synchronized(JVM_LOCK) {
        dir.mkdirs()
        RandomAccessFile(File(dir, ".lock"), "rw").use { f -> f.channel.lock().use { block() } }
    }

    private fun file(id: String): File {
        require(ID.matches(id)) { "a computer id is letters, digits and - only" }
        return File(dir, "$id.peer")
    }

    /** Adds a computer, or updates its key. A known computer with the same key keeps its window. */
    fun set(id: String, boxPublic: ByteArray, newWindow: () -> String): Unit = locked {
        val same = read(id)?.boxPublic?.contentEquals(boxPublic) == true
        if (!same) write(id, boxPublic, newWindow())
    }

    fun remove(id: String): Unit = locked { file(id).delete() }

    fun saveState(id: String, state: String) {
        val peer = read(id) ?: return
        write(id, peer.boxPublic, state)
    }

    fun all(): List<Peer> = dir.listFiles { f -> f.name.endsWith(".peer") }.orEmpty().sortedBy { it.name }.mapNotNull { read(it.name.removeSuffix(".peer")) }

    private fun read(id: String): Peer? {
        val f = file(id)
        if (!f.exists()) return null
        val text = f.readText()
        val key = text.substringBefore('\n')
        val state = text.substringAfter('\n', "")
        if (key.length != 64 || state.isEmpty()) return null
        return Peer(id, key.chunked(2).map { it.toInt(16).toByte() }.toByteArray(), state)
    }

    private fun write(id: String, boxPublic: ByteArray, state: String) {
        dir.mkdirs()
        val tmp = File(dir, "$id.tmp")
        tmp.writeText(boxPublic.joinToString("") { "%02x".format(it) } + "\n" + state)
        if (!tmp.renameTo(file(id))) throw java.io.IOException("could not store the computer")
    }

    companion object {
        private val ID = Regex("[A-Za-z0-9-]{1,64}")
        private val JVM_LOCK = Any()
    }
}
