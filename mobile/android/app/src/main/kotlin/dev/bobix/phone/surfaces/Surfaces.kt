package dev.bobix.phone.surfaces

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.service.quicksettings.TileService
import dev.bobix.phone.MainActivity
import dev.bobix.phone.push.PushPrefs
import dev.bobix.phone.surfaces.widgets.LargeWidget
import dev.bobix.phone.surfaces.widgets.LargeWidgetReceiver
import dev.bobix.phone.surfaces.widgets.SmallWidget
import dev.bobix.phone.surfaces.widgets.SmallWidgetReceiver
import dev.bobix.phone.surfaces.widgets.WideWidget
import dev.bobix.phone.surfaces.widgets.WideWidgetReceiver
import androidx.glance.appwidget.updateAll
import java.io.File
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking

/**
 * The one place that knows every surface outside the app. [refresh] redraws all of them from the shared list; it is
 * called when the app writes the list, when a push changes it, when "Hide details" changes, and by an alarm at the moment
 * the list turns stale. It never answers anything: every tap opens the app, and the answer still needs the fingerprint.
 */
object Surfaces {
    const val ACTION_OPEN = "dev.bobix.phone.OPEN_SURFACE"
    const val EXTRA_TARGET = "target"
    const val EXTRA_COMPUTER = "computer"
    const val EXTRA_ID = "id"
    const val EXTRA_TOKEN = "token"

    fun dir(context: Context) = File(context.noBackupFilesDir, "surfaces")

    fun store(context: Context) = SurfaceStore(dir(context))

    fun view(context: Context, now: Long = System.currentTimeMillis()): SurfaceView =
        SurfaceModel.of(store(context).read(), now, PushPrefs(context).hideDetails)

    /** The intent every tap uses: open the app on [target] (`home`, `start`, or `request` with the item), signed with the app's secret. */
    fun openIntent(context: Context, target: String, computer: String = "", id: String = ""): Intent =
        Intent(context, MainActivity::class.java)
            .setAction(ACTION_OPEN)
            .setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            .putExtra(EXTRA_TARGET, target)
            .putExtra(EXTRA_COMPUTER, computer)
            .putExtra(EXTRA_ID, id)
            .putExtra(EXTRA_TOKEN, PushPrefs(context).tapToken)

    /** The tap that opens the first waiting request, or Home when nothing waits (or the data is stale). */
    fun openFirst(context: Context, v: SurfaceView): Intent {
        val f = v.first
        return if (f == null) openIntent(context, "home")
        else openIntent(context, "request", store(context).read()?.computerId.orEmpty(), f.id)
    }

    fun pending(context: Context, intent: Intent, code: Int): PendingIntent =
        PendingIntent.getActivity(context, code, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)

    /** Redraws everything. Blocks until the widgets are updated: call it off the main thread. */
    fun refresh(context: Context) {
        val app = context.applicationContext
        val v = view(app)
        SurfaceTick.bump()
        runBlocking {
            runCatching { SmallWidget().updateAll(app) }
            runCatching { WideWidget().updateAll(app) }
            runCatching { LargeWidget().updateAll(app) }
        }
        LiveUpdate.apply(app, v)
        for (tile in listOf(BobixTile::class.java, StartTile::class.java)) {
            runCatching { TileService.requestListeningState(app, ComponentName(app, tile)) }
        }
        scheduleStale(app)
    }

    /** After a push: the list changes by the notice, then every surface follows. */
    fun onNotice(context: Context, computerId: String, what: String, ref: String) {
        val now = System.currentTimeMillis()
        // Nothing paired (no list yet): a notice has nothing to change and must not make a list out of nothing.
        store(context).update { it?.withNotice(computerId, what, ref, now) }
        refresh(context)
    }

    /**
     * The alarm that redraws everything at the moment the list turns stale. Inexact on purpose (`set`, not an exact alarm: no
     * special permission, and the system may deliver it a few minutes late; the surfaces judge staleness from the clock
     * whenever they draw, so a late alarm only delays the redraw). It does not survive a reboot or an app update by itself:
     * [StaleReceiver] sets it again at boot, and every widget update and every tile listen sets it too.
     */
    fun scheduleStale(context: Context) {
        val alarms = context.getSystemService(AlarmManager::class.java) ?: return
        val pi = PendingIntent.getBroadcast(
            context, 7, Intent(context, StaleReceiver::class.java), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val list = store(context).read()
        val at = list?.let { SurfaceModel.staleAt(it, System.currentTimeMillis()) }
        if (at == null) alarms.cancel(pi) else alarms.set(AlarmManager.RTC, at + 1000, pi)
    }

    fun widgetReceivers() = listOf(SmallWidgetReceiver::class.java, WideWidgetReceiver::class.java, LargeWidgetReceiver::class.java)
}

/** A counter the widgets observe: a running Glance session is recomposed by it, since `updateAll` does not re-run `provideGlance` there. */
object SurfaceTick {
    val version = kotlinx.coroutines.flow.MutableStateFlow(0L)
    fun bump() = version.update { it + 1 }
}

/** Redraws every surface when the list has become stale, and after a reboot (the alarm is gone then, so this sets it again). */
class StaleReceiver : android.content.BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val pending = goAsync()
        Thread {
            try {
                Surfaces.refresh(context)
            } finally {
                pending.finish()
            }
        }.start()
    }
}
