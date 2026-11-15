package dev.bobix.phone.surfaces

import java.io.File
import org.json.JSONArray
import org.json.JSONObject

/**
 * The small list every surface outside the app reads (home screen widgets, the Live Update, the Quick Settings tiles).
 * The app writes it (`lib/surfaces`) from what the computer last said; the push service updates it when a notice arrives.
 * It holds what waits, by name, and counts: never a command, a tool input or a plan. Nothing in here is protocol: it is
 * bookkeeping over data the app already decided to show.
 */
class SurfaceList(
    val writtenAt: Long,
    val computerId: String,
    val computer: String,
    /** The computer is reachable now (the link's state was fine when this was written). */
    val online: Boolean,
    val todo: Int,
    val waiting: List<Waiting>,
    val sessions: List<Row>,
) {
    /** A pet as the app draws it: the image file name is [petFile]. */
    class Pet(val design: String, val color: Int, val state: String) {
        val petFile get() = "$design-$color-$state.png"
    }

    class Waiting(
        /** The id the app opens (the request, plan or question). */
        val id: String,
        /** `request`, `plan` or `question`. */
        val kind: String,
        /** The session's name; empty when only a push told us (the surface then says "a session"). */
        val name: String,
        val waitMinutes: Int,
        /** Seconds the computer still waits for an answer, as of [SurfaceList.writtenAt]; null when it waits for ever. */
        val leftSeconds: Int?,
        val pet: Pet?,
        /** When this item was last confirmed, as of which [leftSeconds] counts; null: when the list was written. */
        val at: Long? = null,
    )

    /** A session that does not wait, for the large widget. */
    class Row(val name: String, val pet: Pet?)

    fun toJson(): String = JSONObject()
        .put("v", 1)
        .put("writtenAt", writtenAt)
        .put("computerId", computerId)
        .put("computer", computer)
        .put("online", online)
        .put("todo", todo)
        .put("waiting", JSONArray(waiting.map { w ->
            JSONObject().put("id", w.id).put("kind", w.kind).put("name", w.name).put("waitMinutes", w.waitMinutes)
                .put("leftSeconds", w.leftSeconds ?: JSONObject.NULL).put("pet", petJson(w.pet))
                .apply { w.at?.let { put("at", it) } }
        }))
        .put("sessions", JSONArray(sessions.map { JSONObject().put("name", it.name).put("pet", petJson(it.pet)) }))
        .toString()

    private fun petJson(p: Pet?): Any = p?.let { JSONObject().put("design", it.design).put("color", it.color).put("state", it.state) } ?: JSONObject.NULL

    /** When [w] was last confirmed. */
    fun confirmedAt(w: Waiting): Long = w.at ?: writtenAt

    /**
     * After a push: a `waiting` notice adds its request (or refreshes it), a `closed` one removes it; `done` changes
     * nothing. Only the item a notice names is newer: [writtenAt] and every other item keep their age, so a notice does not
     * make the rest of an old list look fresh.
     */
    fun withNotice(computerId: String, what: String, ref: String, now: Long): SurfaceList = when (what) {
        "waiting" -> SurfaceList(
            writtenAt, this.computerId.ifEmpty { computerId }, computer, online, todo,
            waiting.filter { it.id != ref } + Waiting(ref, "request", "", 0, null, null, now), sessions,
        )
        "closed" -> SurfaceList(writtenAt, this.computerId, computer, online, todo, waiting.filter { it.id != ref }, sessions)
        else -> this
    }

    companion object {
        fun parse(text: String): SurfaceList? = try {
            val j = JSONObject(text)
            if (j.getInt("v") != 1) null else SurfaceList(
                writtenAt = j.getLong("writtenAt"),
                computerId = j.optString("computerId"),
                computer = j.optString("computer"),
                online = j.optBoolean("online"),
                todo = j.optInt("todo"),
                waiting = j.optJSONArray("waiting").objects().map {
                    Waiting(
                        it.getString("id"), it.optString("kind", "request"), it.optString("name"), it.optInt("waitMinutes"),
                        if (it.isNull("leftSeconds")) null else it.getInt("leftSeconds"), pet(it.optJSONObject("pet")),
                        if (it.has("at")) it.getLong("at") else null,
                    )
                },
                sessions = j.optJSONArray("sessions").objects().map { Row(it.optString("name"), pet(it.optJSONObject("pet"))) },
            )
        } catch (e: org.json.JSONException) {
            null
        }

        private fun pet(j: JSONObject?) = j?.let { Pet(it.getString("design"), it.getInt("color"), it.getString("state")) }

        private fun JSONArray?.objects(): List<JSONObject> = if (this == null) emptyList() else (0 until length()).map { getJSONObject(it) }
    }
}

/** The list and the pet images in the app's private, never backed up, files. */
class SurfaceStore(private val dir: File) {
    private val list = File(dir, "list.json")

    fun read(): SurfaceList? = try {
        if (list.exists()) SurfaceList.parse(list.readText()) else null
    } catch (e: java.io.IOException) {
        null
    }

    fun write(text: String): Boolean {
        if (SurfaceList.parse(text) == null) return false
        return locked { put(text) }
    }

    /** Reads the list, lets [change] make the new one (null: leave it), and writes it, with nobody else writing in between. */
    fun update(change: (SurfaceList?) -> SurfaceList?): Boolean = locked {
        val next = change(read()) ?: return@locked false
        put(next.toJson())
    }

    private fun put(text: String): Boolean {
        dir.mkdirs()
        val tmp = File(dir, "list-${System.nanoTime()}.tmp")
        tmp.writeText(text)
        return tmp.renameTo(list).also { if (!it) tmp.delete() }
    }

    /** One writer at a time: inside this process by a monitor, and across processes by a lock on a file. */
    private fun <T> locked(block: () -> T): T = synchronized(LOCK) {
        dir.mkdirs()
        java.io.RandomAccessFile(File(dir, "list.lock"), "rw").use { f -> f.channel.lock().use { block() } }
    }

    fun clear() {
        locked {
            list.delete()
            File(dir, "pets").deleteRecursively()
        }
    }

    fun petFile(name: String): File? = File(dir, "pets/$name").takeIf { PET_NAME.matches(name) && it.exists() }

    fun writePet(name: String, png: ByteArray): Boolean {
        if (!PET_NAME.matches(name) || png.size > MAX_PNG) return false
        val pets = File(dir, "pets").apply { mkdirs() }
        val tmp = File(pets, "$name.tmp")
        tmp.writeBytes(png)
        return tmp.renameTo(File(pets, name))
    }

    companion object {
        private val LOCK = Any()
        private val PET_NAME = Regex("[a-z0-9-]{1,40}-[0-9]{1,3}-[a-z]{1,20}\\.png")
        private const val MAX_PNG = 200 * 1024
    }
}
