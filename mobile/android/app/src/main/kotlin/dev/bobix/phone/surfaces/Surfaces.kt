package dev.bobix.phone.surfaces

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.service.quicksettings.TileService
import dev.bobix.phone.MainActivity
import dev.bobix.phone.push.PushPrefs
import dev.bobix.phone.surfaces.widgets.LargeWidgetReceiver
import dev.bobix.phone.surfaces.widgets.SmallWidgetReceiver
import dev.bobix.phone.surfaces.widgets.WideWidgetReceiver
import androidx.glance.appwidget.updateAll
import java.io.File
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
    const val EXTRA_KIND = "kind"
    const val EXTRA_TOKEN = "token"

    fun dir(context: Context) = File(context.noBackupFilesDir, "surfaces")

    fun store(context: Context) = SurfaceStore(dir(context))

    fun view(context: Context, now: Long = System.currentTimeMillis()): SurfaceView =
        SurfaceModel.of(store(context).read(), now, PushPrefs(context).hideDetails)

    /** The intent every tap uses: open the app on [target] (`home`, `start`, or `request` with the item), signed with the app's secret. */
    fun openIntent(context: Context, target: String, computer: String = "", id: String = "", kind: String = ""): Intent =
        Intent(context, MainActivity::class.java)
            .setAction(ACTION_OPEN)
            .setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            .putExtra(EXTRA_TARGET, target)
            .putExtra(EXTRA_COMPUTER, computer)
            .putExtra(EXTRA_ID, id)
            .putExtra(EXTRA_KIND, kind)
            .putExtra(EXTRA_TOKEN, PushPrefs(context).tapToken)

    /** The tap that opens the first waiting request, or Home when nothing waits (or the data is stale). */
    fun openFirst(context: Context, v: SurfaceView): Intent {
        val f = v.first
        return if (f == null) openIntent(context, "home")
        else openIntent(context, "request", store(context).read()?.computerId.orEmpty(), f.id, f.kind)
    }

    fun pending(context: Context, intent: Intent, code: Int): PendingIntent =
        PendingIntent.getActivity(context, code, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)

    /** Redraws everything. Blocks until the widgets are updated: call it off the main thread. */
    fun refresh(context: Context) {
        val app = context.applicationContext
        val v = view(app)
        runBlocking {
            runCatching { dev.bobix.phone.surfaces.widgets.BobixWidget(dev.bobix.phone.surfaces.widgets.WidgetSize.SMALL).updateAll(app) }
            runCatching { dev.bobix.phone.surfaces.widgets.BobixWidget(dev.bobix.phone.surfaces.widgets.WidgetSize.WIDE).updateAll(app) }
            runCatching { dev.bobix.phone.surfaces.widgets.BobixWidget(dev.bobix.phone.surfaces.widgets.WidgetSize.LARGE).updateAll(app) }
        }
        LiveUpdate.apply(app, v)
        for (tile in listOf(BobixTile::class.java, StartTile::class.java)) {
            runCatching { TileService.requestListeningState(app, ComponentName(app, tile)) }
        }
        scheduleStale(app)
    }

    /** After a push: the list changes by the notice, then every surface follows. */
    fun onNotice(context: Context, computerId: String, what: String, ref: String) {
        val store = store(context)
        val now = System.currentTimeMillis()
        val list = store.read() ?: SurfaceList(now, computerId, "", true, 0, emptyList(), emptyList())
        store.write(list.withNotice(computerId, what, ref, now).toJson())
        refresh(context)
    }

    /** The alarm that redraws everything at the moment the list turns stale (inexact: a few minutes late is fine). */
    private fun scheduleStale(context: Context) {
        val alarms = context.getSystemService(AlarmManager::class.java) ?: return
        val pi = PendingIntent.getBroadcast(
            context, 7, Intent(context, StaleReceiver::class.java), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val list = store(context).read()
        val at = list?.let { SurfaceModel.staleAt(it) }
        if (at == null || at <= System.currentTimeMillis()) alarms.cancel(pi) else alarms.set(AlarmManager.RTC, at + 1000, pi)
    }

    fun widgetReceivers() = listOf(SmallWidgetReceiver::class.java, WideWidgetReceiver::class.java, LargeWidgetReceiver::class.java)
}

/** Redraws every surface when the list has become stale. */
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
