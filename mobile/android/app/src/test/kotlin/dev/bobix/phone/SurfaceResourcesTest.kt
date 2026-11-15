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

    private fun kotlin(path: String) = File(main, "kotlin/dev/bobix/phone/$path").readText()

    @Test
    fun nothingRelaySuppliedIsDrawnWhenTheSealedBoxDoesNotOpen() {
        // The list changes by a push only for a notice that opened; its title and body are never copied into it.
        val service = kotlin("push/PushService.kt")
        assertTrue(service.contains("opened?.let { dev.bobix.phone.surfaces.Surfaces.onNotice(context, it.computerId, it.notice.what, it.notice.reference) }"))
        assertTrue("the opener returns null for a box that does not open", service.contains(".open(sealed)"))
        val list = kotlin("surfaces/SurfaceList.kt")
        assertTrue(!list.contains(".title") && !list.contains(".body"))
        // A surface draws names only from the list the app wrote, or the generic text.
        assertTrue(kotlin("surfaces/Surfaces.kt").contains("fun onNotice(context: Context, computerId: String, what: String, ref: String)"))
    }

    @Test
    fun eachWidgetSizeIsItsOwnClassAndReadsTheListInsideTheComposition() {
        val w = kotlin("surfaces/widgets/BobixWidget.kt")
        for (n in listOf("Small", "Wide", "Large")) {
            assertTrue("class ${n}Widget", w.contains("class ${n}Widget : BobixWidget(WidgetSize.${n.uppercase()})"))
            assertTrue("${n}WidgetReceiver uses ${n}Widget()", w.contains("override val glanceAppWidget: GlanceAppWidget = ${n}Widget()"))
        }
        // A running Glance session is not re-run by updateAll: the list is read in the composition, keyed on a counter.
        assertTrue(w.contains("SurfaceTick.version.collectAsState()") && w.contains("remember(tick)"))
        assertTrue(kotlin("surfaces/Surfaces.kt").contains("SurfaceTick.bump()"))
    }

    @Test
    fun theStaleAlarmIsSetAgainAfterABootAndByTheSurfacesThemselves() {
        val manifest = File(main, "AndroidManifest.xml").readText()
        assertTrue(manifest.contains("android.permission.RECEIVE_BOOT_COMPLETED") && manifest.contains("android.intent.action.BOOT_COMPLETED"))
        assertTrue(kotlin("surfaces/widgets/BobixWidget.kt").contains("Surfaces.scheduleStale(context)"))
        assertTrue(kotlin("surfaces/Tiles.kt").contains("Surfaces.scheduleStale(this)"))
        assertTrue(kotlin("surfaces/LiveUpdate.kt").split("setTimeoutAfter").size == 3)
    }

    @Test
    fun hideDetailsRedrawsEverySurfaceAtOnce() {
        assertTrue(kotlin("push/PushChannel.kt").contains("prefs.hideDetails = call.argument<Boolean>(\"hide\") ?: false"))
        assertTrue(kotlin("push/PushChannel.kt").contains("Surfaces.refresh(activity.applicationContext)"))
        val refresh = kotlin("surfaces/Surfaces.kt")
        for (part in listOf("SmallWidget()", "WideWidget()", "LargeWidget()", "LiveUpdate.apply", "requestListeningState")) assertTrue("refresh covers $part", refresh.contains(part))
        for (file in listOf("surfaces/LiveUpdate.kt", "surfaces/Tiles.kt", "surfaces/widgets/BobixWidget.kt")) {
            assertTrue("$file reads the switch through the shared view", kotlin(file).contains("Surfaces.view") || kotlin(file).contains("v.hidden") || kotlin(file).contains("SurfaceView"))
        }
    }

    @Test
    fun aTapOnlyOpensAScreenAndGoesThroughTheApp() {
        // Every surface taps through one intent to MainActivity, signed with the app's secret; no surface code touches keys,
        // signing, the relay or a command, so the answer still needs the fingerprint inside the app.
        val forbidden = listOf("KeyStore", "BiometricPrompt", "SecureKeys", "makeGrant", "openPacket", "sign(", "Signature")
        val dir = File(main, "kotlin/dev/bobix/phone/surfaces")
        for (f in dir.walkTopDown().filter { it.extension == "kt" }) {
            for (w in forbidden) assertTrue("${f.name} must not use $w", !f.readText().contains(w))
        }
        assertTrue(kotlin("surfaces/SurfacesChannel.kt").contains("EXTRA_TOKEN) != PushPrefs(context).tapToken"))
        assertTrue(kotlin("surfaces/Surfaces.kt").contains("Intent(context, MainActivity::class.java)"))
    }
}
