package dev.bobix.phone

import dev.bobix.phone.surfaces.SurfaceList
import dev.bobix.phone.surfaces.SurfaceModel
import dev.bobix.phone.surfaces.SurfaceState
import dev.bobix.phone.surfaces.SurfaceStore
import java.nio.file.Files
import java.util.TimeZone
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The list the surfaces read and the rule that decides what each of them shows. All names and times are made up. */
class SurfaceModelTest {
    private val min = 60_000L
    private val t0 = 1_800_000_000_000L
    private val utc = TimeZone.getTimeZone("UTC")

    private fun list(
        at: Long = t0,
        online: Boolean = true,
        waiting: List<SurfaceList.Item> = emptyList(),
        sessions: List<SurfaceList.Item> = listOf(SurfaceList.Item("s1", "api", "web", "working", "default", 3, "", "c1", 900, 0)),
    ) = SurfaceList(at, "Studio Mac (demo)", if (online) "online" else "offline", 2, true, waiting + sessions)

    private fun waits(left: Int = 900, name: String = "demo-app", id: String = "r1") =
        SurfaceList.Item("s-$id", name, "web", "waiting", "default", 3, id, "c1", left, 0)

    @Test
    fun noListMeansNothingIsPaired() {
        val v = SurfaceModel.of(null, t0, hideDetails = false, zone = utc)
        assertEquals(SurfaceState.UNPAIRED, v.state)
        assertEquals(0, v.count)
        assertNull(v.first)
    }

    @Test
    fun freshDataSaysWhatWaitsOrThatNothingDoes() {
        val needs = SurfaceModel.of(list(waiting = listOf(waits())), t0 + 5 * min, false, utc)
        assertEquals(SurfaceState.NEEDS_YOU, needs.state)
        assertEquals(1, needs.count)
        assertEquals("demo-app", needs.rows.single().name)
        assertEquals("r1", needs.first?.id)
        assertEquals("Studio Mac (demo)", needs.computer)
        assertEquals(1, needs.others.size)
        val none = SurfaceModel.of(list(), t0, false, utc)
        assertEquals(SurfaceState.NOTHING, none.state)
        assertNull(none.first)
    }

    @Test
    fun contentOlderThanFifteenMinutesIsStaleWhateverTheLinkSaid() {
        for (online in listOf(true, false)) {
            val l = list(online = online, waiting = listOf(waits()))
            assertEquals(SurfaceState.NEEDS_YOU, SurfaceModel.of(l, t0 + 14 * min, false, utc).state)
            assertEquals("online=$online", SurfaceState.STALE, SurfaceModel.of(l, t0 + 16 * min, false, utc).state)
            assertEquals(t0 + 15 * min, SurfaceModel.staleAt(l, t0))
        }
        assertEquals("nothing waiting is stale too", SurfaceState.STALE, SurfaceModel.of(list(), t0 + 16 * min, false, utc).state)
    }

    @Test
    fun aWaitShorterThanFifteenMinutesDimsAtTheWait() {
        val l = list(waiting = listOf(waits(left = 120)))
        assertEquals(SurfaceState.NEEDS_YOU, SurfaceModel.of(l, t0 + 90_000, false, utc).state)
        val stale = SurfaceModel.of(l, t0 + 130_000, false, utc)
        assertEquals(SurfaceState.STALE, stale.state)
        assertEquals(t0 + 120_000, SurfaceModel.staleAt(l, t0))
        // The earliest of the two wins: a long wait still dims at fifteen minutes.
        assertEquals(t0 + 15 * min, SurfaceModel.staleAt(list(waiting = listOf(waits(left = 3600))), t0))
    }

    @Test
    fun staleDataNeverSaysNeedsYouAndShowsWhenItWasWritten() {
        val v = SurfaceModel.of(list(at = t0, waiting = listOf(waits(), waits(name = "api", id = "r2"))), t0 + 20 * min, false, utc)
        assertEquals(SurfaceState.STALE, v.state)
        assertEquals(0, v.count)
        assertTrue(v.rows.isEmpty() && v.others.isEmpty())
        assertNull("a stale surface opens Home, not a request", v.first)
        assertEquals("08:00", v.updatedAt) // 1_800_000_000 s is 2027-01-15 08:00:00 UTC
    }

    @Test
    fun hideDetailsLeavesCountsOnly() {
        val v = SurfaceModel.of(list(waiting = listOf(waits(), waits(name = "api", id = "r2"))), t0, hideDetails = true, zone = utc)
        assertEquals(2, v.count)
        assertTrue(v.rows.isEmpty() && v.others.isEmpty())
        assertEquals("", v.computer)
        assertTrue(v.hidden)
    }

    @Test
    fun theListHoldsNoCommandOrToolText() {
        val keys = mutableSetOf<String>()
        fun walk(o: Any?) {
            when (o) {
                is JSONObject -> o.keys().forEach { keys += it; walk(o.get(it)) }
                is org.json.JSONArray -> (0 until o.length()).forEach { walk(o.get(it)) }
            }
        }
        walk(JSONObject(list(waiting = listOf(waits())).toJson()))
        assertEquals(
            setOf("written", "computerName", "computerState", "todo", "canStart", "rows", "id", "name", "project", "state", "pet", "color", "ref", "computer", "holdSecs", "at"),
            keys,
        )
    }

    @Test
    fun theListSurvivesAWriteAndARead() {
        val original = list(waiting = listOf(waits(left = 300)))
        val back = SurfaceList.parse(original.toJson())!!
        assertEquals(original.toJson(), back.toJson())
        assertEquals(300, back.waiting.single().holdSecs)
        assertEquals("default-3-waiting.png", back.waiting.single().pet?.petFile)
        assertNull(SurfaceList.parse("{"))
        assertNull(SurfaceList.parse("{}"))
        assertNull(SurfaceList.parse("""{"written":1,"rows":[{"id":"a"}]}"""))
    }

    @Test
    fun aPushChangesTheListByItsNotice() {
        val l = list(waiting = listOf(waits()))
        val more = l.withNotice("c1", "waiting", "r2", t0 + 1000)
        assertEquals(listOf("r1", "r2"), more.waiting.map { it.id }.sorted())
        assertEquals("", more.waiting.first { it.id == "r2" }.name)
        assertEquals("a second waiting for the same ref replaces it", 2, more.withNotice("c1", "waiting", "r2", t0).waiting.size)
        assertEquals(listOf("r2"), more.withNotice("c1", "closed", "r1", t0).waiting.map { it.id })
        assertEquals(l.toJson(), l.withNotice("c1", "done", "r1", t0 + 5).toJson())
        assertEquals("a notice does not make the list newer", t0, more.writtenAt)
        assertEquals(t0 + 1000, more.waiting.first { it.id == "r2" }.at)
    }

    @Test
    fun aNoticeDoesNotRefreshTheAgeOfTheRestOfTheList() {
        val old = list(waiting = listOf(waits()))
        val withNew = old.withNotice("c1", "waiting", "r2", t0 + 14 * min)
        val at16 = SurfaceModel.of(withNew, t0 + 16 * min, false)
        assertEquals(SurfaceState.NEEDS_YOU, at16.state)
        assertEquals("only the item the notice named is fresh", 1, at16.count)
        assertEquals("r2", at16.first!!.id)
        assertEquals(SurfaceState.STALE, SurfaceModel.of(withNew, t0 + 30 * min, false).state)
        assertEquals("the alarm is for the next item to turn stale", t0 + 15 * min, SurfaceModel.staleAt(withNew, t0 + min))
        val roundTrip = SurfaceList.parse(withNew.toJson())!!
        assertEquals(t0 + 14 * min, roundTrip.waiting.first { it.id == "r2" }.at)
    }

    @Test
    fun twoWritersLeaveBothChangesAndNoTempFile() {
        val dir = Files.createTempDirectory("surfaces").toFile()
        try {
            val store = SurfaceStore(dir)
            store.write(list().toJson())
            val threads = (1..8).map { n -> Thread { repeat(5) { k -> store.update { it!!.withNotice("c1", "waiting", "r$n-$k", t0 + n) } } } }
            threads.forEach { it.start() }
            threads.forEach { it.join() }
            assertEquals(40, store.read()!!.waiting.size)
            assertEquals(emptyList<String>(), dir.list()!!.filter { it.endsWith(".tmp") })
            assertFalse("nothing to update, nothing written", SurfaceStore(Files.createTempDirectory("none").toFile()).update { it })
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun theStoreWritesWholeFilesAndRefusesWhatIsNotAList() {
        val dir = Files.createTempDirectory("surfaces").toFile()
        try {
            val store = SurfaceStore(dir)
            assertNull(store.read())
            assertFalse(store.write("not json"))
            assertTrue(store.write(list(waiting = listOf(waits())).toJson()))
            assertNotNull(store.read())
            assertTrue(store.writePet("default-3-waiting.png", ByteArray(10)))
            assertNotNull(store.petFile("default-3-waiting.png"))
            assertFalse("a name that is not a pet file", store.writePet("../escape.png", ByteArray(1)))
            assertFalse(store.writePet("default-3-waiting.png", ByteArray(300 * 1024)))
            assertNull(store.petFile("../../list.json"))
            store.clear()
            assertNull(store.read())
            assertNull(store.petFile("default-3-waiting.png"))
        } finally {
            dir.deleteRecursively()
        }
    }
}
