package dev.bobix.phone.surfaces

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import dev.bobix.phone.R

/**
 * An ongoing notification while something waits: a Live Update on Android 16 (a promoted ongoing, progress style), an
 * ordinary ongoing one with an indeterminate bar below it. "Open to review" opens the first waiting request; nothing
 * answers. Private on the lock screen: the public version says only that a session needs you. It is removed when nothing
 * waits, when the computer is gone, and when its data is stale (it never claims "needs you" on old data).
 */
object LiveUpdate {
    const val CHANNEL = "live_update"
    const val ID = 4521

    fun apply(context: Context, v: SurfaceView) {
        val manager = NotificationManagerCompat.from(context)
        if (v.state != SurfaceState.NEEDS_YOU || !manager.areNotificationsEnabled()) {
            manager.cancel(ID)
            return
        }
        ensureChannel(context)
        val open = Surfaces.pending(context, Surfaces.openFirst(context, v), 21)
        val title = context.getString(R.string.phone_widget_waiting, v.count.toString())
        val detail = v.rows.firstOrNull()?.name?.takeUnless { v.hidden } ?: context.getString(R.string.phone_push_generic)
        val publicVersion = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(context.getString(R.string.phone_push_generic))
            .setContentText(context.getString(R.string.phone_push_generic_body))
            .build()
        val action = context.getString(R.string.phone_widget_open)
        val notification: Notification = if (Build.VERSION.SDK_INT >= 36) {
            promoted(context, title, detail, publicVersion, open, action, v.count)
        } else {
            NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(title)
                .setContentText(detail)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setProgress(0, 0, true)
                .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
                .setPublicVersion(publicVersion)
                .setCategory(NotificationCompat.CATEGORY_STATUS)
                .setContentIntent(open)
                .addAction(0, action, open)
                .build()
        }
        manager.notify(ID, notification)
    }

    @androidx.annotation.RequiresApi(36)
    private fun promoted(
        context: Context,
        title: String,
        detail: String,
        publicVersion: Notification,
        open: android.app.PendingIntent,
        action: String,
        count: Int,
    ): Notification = Notification.Builder(context, CHANNEL)
        .setSmallIcon(R.drawable.ic_notification)
        .setContentTitle(title)
        .setContentText(detail)
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setStyle(Notification.ProgressStyle().setProgressIndeterminate(true))
        .setShortCriticalText(count.toString())
        // `setRequestPromotedOngoing(true)` is the call; its extra is what it sets (the SDK this builds against may predate it).
        .addExtras(android.os.Bundle().apply { putBoolean("android.requestPromotedOngoing", true) })
        .setVisibility(Notification.VISIBILITY_PRIVATE)
        .setPublicVersion(publicVersion)
        .setCategory(Notification.CATEGORY_STATUS)
        .setContentIntent(open)
        .addAction(Notification.Action.Builder(null, action, open).build())
        .build()

    private fun ensureChannel(context: Context) {
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, context.getString(R.string.phone_live_channel), NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = context.getString(R.string.phone_live_channel_d)
                lockscreenVisibility = Notification.VISIBILITY_PRIVATE
                setSound(null, null)
                enableVibration(false)
            },
        )
    }
}
