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
    /** Content older than this is stale. */
    const val MAX_AGE_MS = 15 * 60_000L

    /**
     * The staleness rule (decision 24): content older than 15 minutes is drawn stale, and so is content older than the wait
     * the computer still had for a waiting item (`leftSeconds`: after that the request may be gone), so a wait under 15
     * minutes dims at the wait. Stale means the dimmed "Updated hh:mm" and never "needs you". The app rewrites the list
     * whenever the computer says something and every few minutes while the computer is reachable, so a reachable computer
     * does not go stale while the app runs; a phone that cannot reach it, or an app that is not running, does.
     */
    fun staleAfter(w: SurfaceList.Waiting): Long = minOf(w.leftSeconds?.let { it * 1000L } ?: Long.MAX_VALUE, MAX_AGE_MS).coerceAtLeast(0)

    /** An item is stale when it was confirmed longer ago than [staleAfter]; the list's "nothing waits" when it was written longer ago than [MAX_AGE_MS]. */
    private fun fresh(list: SurfaceList, w: SurfaceList.Waiting, now: Long) = now - list.confirmedAt(w) <= staleAfter(w)

    /** The next moment something in the list turns stale (the alarm's time), or null when nothing will. */
    fun staleAt(list: SurfaceList, now: Long): Long? {
        val items = list.waiting.map { list.confirmedAt(it) + staleAfter(it) }
        val all = if (list.waiting.isEmpty()) items + (list.writtenAt + MAX_AGE_MS) else items
        return all.filter { it >= now }.minOrNull()
    }

    fun clock(at: Long, zone: TimeZone = TimeZone.getDefault(), locale: Locale = Locale.getDefault()): String =
        SimpleDateFormat("HH:mm", locale).apply { timeZone = zone }.format(Date(at))

    fun of(list: SurfaceList?, now: Long, hideDetails: Boolean, zone: TimeZone = TimeZone.getDefault()): SurfaceView {
        if (list == null) return SurfaceView(SurfaceState.UNPAIRED, 0, emptyList(), emptyList(), "", "", hideDetails, null)
        val live = list.waiting.filter { fresh(list, it, now) }
        val state = when {
            live.isNotEmpty() -> SurfaceState.NEEDS_YOU
            list.waiting.isEmpty() && now - list.writtenAt <= MAX_AGE_MS -> SurfaceState.NOTHING
            else -> SurfaceState.STALE
        }
        val rows = if (state == SurfaceState.NEEDS_YOU && !hideDetails) {
            live.map { SurfaceView.Row(it.name.ifEmpty { null }, it.waitMinutes, it.pet) }
        } else {
            emptyList()
        }
        return SurfaceView(
            state = state,
            count = if (state == SurfaceState.NEEDS_YOU) live.size else 0,
            rows = rows,
            others = if (hideDetails || state == SurfaceState.STALE) emptyList() else list.sessions,
            updatedAt = clock(maxOf(list.writtenAt, list.waiting.maxOfOrNull { list.confirmedAt(it) } ?: 0L), zone),
            computer = if (hideDetails) "" else list.computer,
            hidden = hideDetails,
            first = live.firstOrNull().takeIf { state == SurfaceState.NEEDS_YOU },
        )
    }
}
