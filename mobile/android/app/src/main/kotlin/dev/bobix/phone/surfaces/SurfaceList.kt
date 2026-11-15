package dev.bobix.phone.surfaces

import java.io.File
import org.json.JSONArray
import org.json.JSONObject

/**
 * The shared list every surface outside the app reads (home screen widgets, the Live Update, the Quick Settings tiles): the
 * same JSON the iOS widgets read (`lib/surfaces/shared_list.dart` writes it, `ios/WidgetsShared/SharedList.swift` decodes it).
 * The app writes it from what the computer last said; the push service changes it when a notice arrives. It holds names and
 * counts: never a command, a tool input or a plan.
 */
class SurfaceList(
    /** When the app last wrote the whole list, in unix milliseconds. */
    val writtenAt: Long,
    val computer: String,
    /** `online`, `offline` or `asleep`. */
    val computerState: String,
    val todo: Int,
    val canStart: Boolean,
    val rows: List<Item>,
) {
    /** One session as the app lists it. [ref] is what a tap opens (empty: nothing is held for this phone). */
    class Item(
        val id: String,
        val name: String,
        val project: String,
        /** The pet's state: `waiting`, `working`, `idle`, `failed` or `done`. */
        val state: String,
        val pet: String,
        val color: Int,
        val ref: String,
        val computer: String,
        /** Seconds the computer still holds the request for phones as of [at]; at most 15 minutes count. */
        val holdSecs: Int,
        /** When this row was last confirmed, in unix milliseconds; 0: when the list was written. */
        val at: Long,
    ) {
        val waits get() = state == "waiting" && ref.isNotEmpty()
    }

    /** A pet as the app draws it: the image file name is [petFile]. */
    class Pet(val design: String, val color: Int, val state: String) {
        val petFile get() = "$design-$color-$state.png"
    }

    class Waiting(val id: String, val name: String, val holdSecs: Int, val pet: Pet?, val at: Long)

    /** A session that does not wait, for the large widget. */
    class Row(val name: String, val pet: Pet?)

    val online get() = computerState == "online"
    val computerId get() = rows.firstOrNull { it.computer.isNotEmpty() }?.computer.orEmpty()

    val waiting: List<Waiting>
        get() = rows.filter { it.waits }.map { Waiting(it.ref, it.name, it.holdSecs, pet(it), if (it.at == 0L) writtenAt else it.at) }

    val sessions: List<Row>
        get() = rows.filter { !it.waits }.map { Row(it.name, pet(it)) }

    private fun pet(i: Item) = Pet(i.pet, i.color, i.state)

    fun toJson(): String = JSONObject()
        .put("written", writtenAt)
        .put("computerName", computer)
        .put("computerState", computerState)
        .put("todo", todo)
        .put("canStart", canStart)
        .put("rows", JSONArray(rows.map {
            JSONObject().put("id", it.id).put("name", it.name).put("project", it.project).put("state", it.state).put("pet", it.pet)
                .put("color", it.color).put("ref", it.ref).put("computer", it.computer).put("holdSecs", it.holdSecs).put("at", it.at)
        }))
        .toString()

    /** When [w] was last confirmed. */
    fun confirmedAt(w: Waiting): Long = w.at

    /**
     * After a push: a `waiting` notice adds its request (or refreshes it), a `closed` one removes it; `done` changes
     * nothing. Only the row a notice names is newer: [writtenAt] and every other row keep their age, so a notice does not
     * make the rest of an old list look fresh.
     */
    fun withNotice(computerId: String, what: String, ref: String, now: Long): SurfaceList = when (what) {
        "waiting" -> {
            val old = rows.firstOrNull { it.ref == ref && it.computer == computerId }
            val row = old?.let { Item(it.id, it.name, it.project, "waiting", it.pet, it.color, ref, computerId, HOLD_MAX, now) }
                ?: Item(ref, "", "", "waiting", "default", 0, ref, computerId, HOLD_MAX, now)
            SurfaceList(writtenAt, computer, computerState, todo, canStart, rows.filter { it !== old } + row)
        }
        "closed" -> SurfaceList(writtenAt, computer, computerState, todo, canStart, rows.filterNot { it.ref == ref && it.computer == computerId && it.state == "waiting" })
        else -> this
    }

    companion object {
        /** A request counts as current for at most this long, in seconds. */
        const val HOLD_MAX = 900

        fun parse(text: String): SurfaceList? = try {
            val j = JSONObject(text)
            SurfaceList(
                writtenAt = j.getLong("written"),
                computer = j.optString("computerName"),
                computerState = j.optString("computerState", "offline"),
                todo = j.optInt("todo"),
                canStart = j.optBoolean("canStart"),
                rows = j.optJSONArray("rows").objects().map {
                    Item(
                        it.getString("id"), it.optString("name"), it.optString("project"), it.getString("state"),
                        it.optString("pet", "default"), it.optInt("color"), it.optString("ref"), it.optString("computer"),
                        it.optInt("holdSecs", HOLD_MAX).coerceIn(1, HOLD_MAX), it.optLong("at"),
                    )
                },
            )
        } catch (e: org.json.JSONException) {
            null
        }

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
