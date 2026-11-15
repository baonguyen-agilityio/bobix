package dev.bobix.phone

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The strings the surfaces use exist in English and Vietnamese with the same arguments, and the manifest declares them. */
class SurfaceResourcesTest {
    private val main = File(System.getProperty("app.src.main")!!)

    private fun strings(dir: String): Map<String, String> =
        Regex("""<string name="([a-z0-9_]+)">(.*?)</string>""").findAll(File(main, "res/$dir/strings.xml").readText())
            .associate { it.groupValues[1] to it.groupValues[2] }

    @Test
    fun everySurfaceStringHasBothLanguagesWithTheSameArguments() {
        val en = strings("values")
        val vi = strings("values-vi")
        val keys = listOf(
            "phone_widget_waiting", "phone_widget_nothing", "phone_widget_updated", "phone_widget_refresh", "phone_widget_unpaired",
            "phone_widget_open", "phone_widget_desc", "phone_widget_hidden", "phone_live_channel", "phone_live_channel_d",
            "phone_tile_bobix_label", "phone_tab_start", "phone_min",
        )
        for (k in keys) {
            assertTrue("en $k", en[k]?.isNotEmpty() == true)
            assertTrue("vi $k", vi[k]?.isNotEmpty() == true)
            val args = Regex("%\\d[$]s")
            assertEquals("arguments of $k", args.findAll(en[k]!!).map { it.value }.toSet(), args.findAll(vi[k]!!).map { it.value }.toSet())
        }
        assertTrue(en["phone_widget_updated"]!!.contains("%1\$s"))
        assertTrue(vi["phone_widget_nothing"]!! != en["phone_widget_nothing"]!!)
    }

    @Test
    fun theWidgetsAreThreeSizesAndTheTilesAreBoundToTheSystem() {
        val sizes = mapOf("small" to (2 to 2), "wide" to (4 to 2), "large" to (4 to 4))
        for ((n, wh) in sizes) {
            val x = File(main, "res/xml/widget_$n.xml").readText()
            assertTrue("$n width", x.contains("targetCellWidth=\"${wh.first}\""))
            assertTrue("$n height", x.contains("targetCellHeight=\"${wh.second}\""))
        }
        val m = File(main, "AndroidManifest.xml").readText()
        assertEquals(2, Regex("android.permission.BIND_QUICK_SETTINGS_TILE").findAll(m).count())
        assertTrue(Regex("""\.surfaces\.StaleReceiver"\s+android:exported="false"""").containsMatchIn(m))
        assertTrue(m.contains("POST_PROMOTED_NOTIFICATIONS"))
    }
}
