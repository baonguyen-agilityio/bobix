package dev.bobix.phone.push

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import dev.bobix.phone.MainActivity
import dev.bobix.phone.R

/** The settings the push service reads without the app being open. Written by the app through the `push` channel. */
class PushPrefs(context: Context) {
    private val prefs = context.getSharedPreferences("push", Context.MODE_PRIVATE)

    /** "Hide details": a notification then says only that a session needs you. Off by default. */
    var hideDetails: Boolean
        get() = prefs.getBoolean("hideDetails", false)
        set(v) = prefs.edit().putBoolean("hideDetails", v).apply()

    /** The OS gave the app a new push token while the app was not running; the app registers it on next open. */
    var tokenChanged: Boolean
        get() = prefs.getBoolean("tokenChanged", false)
        set(v) = prefs.edit().putBoolean("tokenChanged", v).apply()
}

/** Carries out a [NoticeAction]: private on the lock screen, with a public version that says nothing but "needs you". */
class NoticePoster(private val context: Context) {
    private val manager = NotificationManagerCompat.from(context)

    fun apply(action: NoticeAction) {
        when (action) {
            is NoticeAction.Remove -> manager.cancel(action.tag, NOTIFICATION_ID)
            is NoticeAction.Show -> show(action)
        }
    }

    private fun show(a: NoticeAction.Show) {
        if (!manager.areNotificationsEnabled()) return
        ensureChannels()
        val channel = if (a.needsYou) CHANNEL_NEEDS else CHANNEL_DONE
        val genericTitle = context.getString(if (a.needsYou) R.string.phone_push_generic else R.string.phone_push_done_generic)
        val genericBody = context.getString(R.string.phone_push_generic_body)
        val generic = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(genericTitle)
            .setContentText(genericBody)
            .build()
        val notification = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(a.title ?: genericTitle)
            .setContentText(a.body ?: genericBody)
            .setStyle(NotificationCompat.BigTextStyle().bigText(a.body ?: genericBody))
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(generic)
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setPriority(if (a.needsYou) NotificationCompat.PRIORITY_HIGH else NotificationCompat.PRIORITY_LOW)
            .setAutoCancel(true)
            .setContentIntent(tapIntent(a))
            .build()
        // The same tag replaces the earlier notification of that request; a `closed` for it removes it.
        manager.notify(a.tag, NOTIFICATION_ID, notification)
    }

    private fun tapIntent(a: NoticeAction.Show): PendingIntent {
        val intent = Intent(context, MainActivity::class.java)
            .setAction(ACTION_OPEN_NOTICE)
            .setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            .putExtra(EXTRA_REF, a.reference)
            .putExtra(EXTRA_WHAT, a.what)
            .putExtra(EXTRA_SESSION, a.session)
        return PendingIntent.getActivity(context, a.tag.hashCode(), intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }

    private fun ensureChannels() {
        val m = context.getSystemService(NotificationManager::class.java)
        m.createNotificationChannel(
            NotificationChannel(CHANNEL_NEEDS, context.getString(R.string.phone_push_channel_needs), NotificationManager.IMPORTANCE_HIGH).apply {
                description = context.getString(R.string.phone_push_channel_needs_d)
                lockscreenVisibility = android.app.Notification.VISIBILITY_PRIVATE
            },
        )
        m.createNotificationChannel(
            NotificationChannel(CHANNEL_DONE, context.getString(R.string.phone_push_channel_done), NotificationManager.IMPORTANCE_LOW).apply {
                description = context.getString(R.string.phone_push_channel_done_d)
                lockscreenVisibility = android.app.Notification.VISIBILITY_PRIVATE
            },
        )
    }

    companion object {
        const val CHANNEL_NEEDS = "needs_you"
        const val CHANNEL_DONE = "done"
        const val NOTIFICATION_ID = 1
        const val ACTION_OPEN_NOTICE = "dev.bobix.phone.OPEN_NOTICE"
        const val EXTRA_REF = "ref"
        const val EXTRA_WHAT = "what"
        const val EXTRA_SESSION = "session"
    }
}
