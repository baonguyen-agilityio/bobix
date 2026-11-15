package dev.bobix.phone

import dev.bobix.phone.push.NoticeAction
import dev.bobix.phone.push.NoticeHandler
import dev.bobix.phone.push.PacketResult
import dev.bobix.phone.push.PeerStore
import dev.bobix.phone.push.planFor
import java.io.File
import java.nio.file.Files
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.bobix_phone_core.OpenNoticeException
import uniffi.bobix_phone_core.openNotice

/**
 * The payload opener, through the real Rust library (built for this computer by `cargo build` in mobile/rust) and the
 * sealed notices of `notice-fixtures.json`, which the Rust test `fixtures_are_current_and_open_as_they_say` keeps current.
 */
class NoticeHandlerTest {
    private val fixtures = JSONObject(File(System.getProperty("notice.fixtures")!!).readText())
    private val boxSecret = hex(fixtures.getString("phoneBoxSecret"))
    private val desktop = hex(fixtures.getString("desktopBoxPublic"))
    private lateinit var dir: File
    private lateinit var peers: PeerStore

    private fun hex(s: String) = s.chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    private fun case(name: String): Pair<String, String> {
        val cases = fixtures.getJSONArray("cases")
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            if (c.getString("name") == name) return c.getString("sealed") to c.getString("outcome")
        }
        error("no fixture named $name")
    }

    private fun handler(secret: ByteArray? = boxSecret) = NoticeHandler(peers) { secret }

    @Before
    fun setUp() {
        dir = Files.createTempDirectory("peers").toFile()
        peers = PeerStore(dir)
        peers.set("desktop-1", desktop, 1, 1) { NoticeHandler.freshWindow() }
    }

    @After
    fun tearDown() {
        dir.deleteRecursively()
    }

    @Test
    fun everyFixtureDoesWhatItSays() {
        val h = handler()
        val cases = fixtures.getJSONArray("cases")
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val got = h.open(c.getString("sealed"))
            val outcome = c.getString("outcome")
            if (outcome in setOf("waiting", "closed", "done")) assertEquals(c.getString("name"), outcome, got?.notice?.what)
            else assertNull(c.getString("name"), got)
        }
    }

    @Test
    fun theOpenerGivesTheReasonForEachRefusal() {
        val cases = fixtures.getJSONArray("cases")
        var state = NoticeHandler.freshWindow()
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val outcome = c.getString("outcome")
            val packet = java.util.Base64.getUrlDecoder().decode(c.getString("sealed"))
            try {
                val opened = openNotice(boxSecret, desktop, packet, state, 1u, 1u)
                assertEquals(c.getString("name"), outcome, opened.what)
                state = opened.state
            } catch (e: OpenNoticeException) {
                assertEquals(c.getString("name"), outcome, e.javaClass.simpleName)
            }
        }
    }

    @Test
    fun aRefusedPushLeavesTheWindowAlone() {
        val h = handler()
        val replayedFirst = case("replay of the first").first
        val first = case("waiting").first
        assertNotNull(h.open(first))
        val before = peers.all().single().state
        assertNull(h.open(replayedFirst))
        assertNull(h.open(case("altered").first))
        assertEquals(before, peers.all().single().state)
    }

    @Test
    fun whatIsNotAPushOfOursIsDropped() {
        val h = handler()
        assertNull(h.open(null))
        assertNull(h.open(""))
        assertNull(h.open("not base64 !!"))
        assertNull(h.open("A".repeat(NoticeHandler.MAX_SEALED + 1)))
        assertNull("no box key yet", handler(secret = null).open(case("waiting").first))
        assertNull("a box key that is not 32 bytes", handler(secret = ByteArray(5)).open(case("waiting").first))
        peers.remove("desktop-1")
        assertNull("no paired computer", h.open(case("waiting").first))
    }

    @Test
    fun aNoticeFromAnotherComputerIsNotOpenedByThisOnesKey() {
        val h = handler()
        assertNull(h.open(case("from another computer").first))
    }

    @Test
    fun twoPushesAtOnceEachUseTheirOwnCounter() {
        // Three notices arrive together (three threads, as the app and the service may run): all must open, none twice.
        val names = listOf("waiting again for the same ref", "closed", "done")
        val h = handler()
        val pool = Executors.newFixedThreadPool(3)
        val start = CountDownLatch(1)
        val results = names.map { n ->
            pool.submit<String?> {
                start.await()
                h.open(case(n).first)?.notice?.what
            }
        }
        start.countDown()
        assertEquals(names.size, results.mapNotNull { it.get() }.size)
        pool.shutdown()
        for (n in names) assertNull("$n again", h.open(case(n).first))
    }

    @Test
    fun theDecisionFollowsTheKindAndTheSwitch() {
        val h = handler()
        val waiting = h.open(case("waiting").first)!!
        val show = planFor(waiting, hideDetails = false) as NoticeAction.Show
        assertEquals("r:desktop-1:r1", show.tag)
        assertEquals("shop-app needs you", show.title)
        assertEquals("Bash wants to run: ls", show.body)
        assertTrue(show.needsYou)

        val hidden = planFor(waiting, hideDetails = true) as NoticeAction.Show
        assertNull("no project name with Hide details", hidden.title)
        assertNull("no command text with Hide details", hidden.body)
        assertEquals("the tap still opens the request", "r1", hidden.reference)
        assertEquals("desktop-1", hidden.computerId)

        val again = planFor(h.open(case("waiting again for the same ref").first)!!, false) as NoticeAction.Show
        assertEquals("a second notice for the same ref has the same tag, so it replaces the first", show.tag, again.tag)

        val closed = planFor(h.open(case("closed").first)!!, false) as NoticeAction.Remove
        assertEquals(show.tag, closed.tag)

        val done = planFor(h.open(case("done").first)!!, false) as NoticeAction.Show
        assertTrue(done.tag != show.tag && !done.needsYou)
    }

    @Test
    fun aFileThatDoesNotParseNeverStopsAPush() {
        File(dir, "broken.peer").writeText("zz".repeat(32) + "\n1 1\n{}")
        File(dir, "short.peer").writeText("00")
        File(dir, "empty.peer").writeText("")
        File(dir, "not valid!.peer").writeText("x")
        assertEquals("the good computer is still used", "waiting", handler().open(case("waiting").first)?.notice?.what)
        assertEquals(1, peers.all().size)
    }

    @Test
    fun openAnyGivesTheAnswersTheChannelNames() {
        val h = handler()
        val waiting = java.util.Base64.getUrlDecoder().decode(case("waiting").first)
        val opened = h.openAny(waiting) as PacketResult.Opened
        assertEquals(Triple("desktop-1", "event", 100L), Triple(opened.computerId, opened.stream, opened.counter))
        assertTrue(opened.plaintext.isNotEmpty())
        assertEquals("replayed", (h.openAny(waiting) as PacketResult.Refused).why)
        assertNull("what the app took is a replay for the push service", h.open(case("waiting").first))
        assertEquals("unreadable", (h.openAny(java.util.Base64.getUrlDecoder().decode(case("from another computer").first)) as PacketResult.Refused).why)
        assertEquals("noKey", (handler(secret = null).openAny(waiting) as PacketResult.Refused).why)
    }

    @Test
    fun theVersionRangeOfTheComputerIsHonoured() {
        peers.set("desktop-1", desktop, 2, 3) { NoticeHandler.freshWindow() }
        assertEquals("a version 1 packet is outside 2..3", "version", (handler().openAny(java.util.Base64.getUrlDecoder().decode(case("waiting").first)) as PacketResult.Refused).why)
    }
}
