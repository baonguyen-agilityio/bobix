package dev.bobix.phone.surfaces

import android.app.PendingIntent
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import dev.bobix.phone.R

/** The "Bobix" tile: the count, and a tap that opens the first waiting request (Home when nothing waits). Never answers. */
class BobixTile : TileService() {
    override fun onStartListening() = draw()

    override fun onClick() {
        val v = Surfaces.view(this)
        launch(Surfaces.openFirst(this, v))
    }

    private fun draw() {
        val tile = qsTile ?: return
        val v = Surfaces.view(this)
        tile.label = getString(R.string.phone_tile_bobix_label)
        tile.icon = Icon.createWithResource(this, R.drawable.ic_notification)
        tile.state = if (v.state == SurfaceState.NEEDS_YOU) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.subtitle = tileText(this, v)
        tile.updateTile()
    }
}

/** The "Start" tile: opens the Start tab. */
class StartTile : TileService() {
    override fun onStartListening() {
        val tile = qsTile ?: return
        tile.label = getString(R.string.phone_tab_start)
        tile.icon = Icon.createWithResource(this, R.drawable.ic_notification)
        tile.state = Tile.STATE_INACTIVE
        tile.subtitle = null
        tile.updateTile()
    }

    override fun onClick() = launch(Surfaces.openIntent(this, "start"))
}

/** What a tile (and the widgets' one-liners) say for each state; the stale one is dimmed by the tile being inactive. */
fun tileText(context: android.content.Context, v: SurfaceView): String = when (v.state) {
    SurfaceState.UNPAIRED -> context.getString(R.string.phone_widget_unpaired)
    SurfaceState.NEEDS_YOU -> context.getString(R.string.phone_widget_waiting, v.count.toString())
    SurfaceState.NOTHING -> context.getString(R.string.phone_widget_nothing)
    SurfaceState.STALE -> context.getString(R.string.phone_widget_updated, v.updatedAt)
}

private fun TileService.launch(intent: Intent) {
    if (Build.VERSION.SDK_INT >= 34) {
        startActivityAndCollapse(PendingIntent.getActivity(this, 22, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
    } else {
        @Suppress("DEPRECATION")
        startActivityAndCollapse(intent)
    }
}
