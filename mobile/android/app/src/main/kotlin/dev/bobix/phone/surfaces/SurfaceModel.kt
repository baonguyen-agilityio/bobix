package dev.bobix.phone.surfaces

import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/** What a surface shows, decided once for all of them (widgets, Live Update, tiles). */
enum class SurfaceState {
    /** Nothing is paired, or nothing was ever written. */
    UNPAIRED,

    /** Fresh data, and something waits. */
    NEEDS_YOU,

    /** Fresh data, and nothing waits. */
    NOTHING,

    /** The data is old: dimmed, "Updated hh:mm", and never "needs you". */
    STALE,
}

class SurfaceView(
    val state: SurfaceState,
    /** How many wait. Never shown while [state] is [SurfaceState.STALE]. */
    val count: Int,
    /** The waiting items, without names when Hide details is on. */
    val rows: List<Row>,
    /** Sessions that do not wait (for the large widget). Empty with Hide details. */
    val others: List<SurfaceList.Row>,
    /** `hh:mm` of when the list was written. */
    val updatedAt: String,
    val computer: String,
    val hidden: Boolean,
    /** The first waiting item, for the tap that opens it. */
    val first: SurfaceList.Waiting?,
) {
    class Row(val name: String?, val minutes: Int, val pet: SurfaceList.Pet?)
}

object SurfaceModel {
    /** Data older than this, with the computer not reachable, is stale. */
    const val STALE_OFFLINE_MS = 15 * 60_000L

    /**
     * The staleness rule: the list is stale when it is older than the wait the computer still had for a waiting item
     * (`leftSeconds`: after that the request may be gone, so it must not still say "needs you"), or older than 15 minutes
     * while the computer was not reachable when it was written. A wait shorter than 15 minutes dims at the wait. Fresh data
     * of a reachable computer with nothing timed stays fresh: the link rewrites the list whenever the computer says
     * something.
     */
    fun staleAfter(list: SurfaceList): Long {
        val own = list.waiting.mapNotNull { it.leftSeconds }.minOrNull()?.let { it * 1000L }
        val offline = if (list.online) Long.MAX_VALUE else STALE_OFFLINE_MS
        return minOf(own ?: Long.MAX_VALUE, offline).coerceAtLeast(0)
    }

    fun isStale(list: SurfaceList, now: Long) = now - list.writtenAt > staleAfter(list)

    /** The moment the list turns stale, or null when it never does on its own. */
    fun staleAt(list: SurfaceList): Long? = staleAfter(list).takeIf { it != Long.MAX_VALUE }?.let { list.writtenAt + it }

    fun clock(at: Long, zone: TimeZone = TimeZone.getDefault(), locale: Locale = Locale.getDefault()): String =
        SimpleDateFormat("HH:mm", locale).apply { timeZone = zone }.format(Date(at))

    fun of(list: SurfaceList?, now: Long, hideDetails: Boolean, zone: TimeZone = TimeZone.getDefault()): SurfaceView {
        if (list == null) return SurfaceView(SurfaceState.UNPAIRED, 0, emptyList(), emptyList(), "", "", hideDetails, null)
        val stale = isStale(list, now)
        val state = when {
            stale -> SurfaceState.STALE
            list.waiting.isNotEmpty() -> SurfaceState.NEEDS_YOU
            else -> SurfaceState.NOTHING
        }
        val rows = if (state == SurfaceState.NEEDS_YOU && !hideDetails) {
            list.waiting.map { SurfaceView.Row(it.name.ifEmpty { null }, it.waitMinutes, it.pet) }
        } else {
            emptyList()
        }
        return SurfaceView(
            state = state,
            count = if (state == SurfaceState.NEEDS_YOU) list.waiting.size else 0,
            rows = rows,
            others = if (hideDetails || state == SurfaceState.STALE) emptyList() else list.sessions,
            updatedAt = clock(list.writtenAt, zone),
            computer = if (hideDetails) "" else list.computer,
            hidden = hideDetails,
            first = list.waiting.firstOrNull().takeIf { state == SurfaceState.NEEDS_YOU },
        )
    }
}
