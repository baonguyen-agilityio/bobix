package dev.bobix.phone.push

import android.content.Context
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dev.bobix.phone.keys.KeyStore
import java.io.File

/**
 * Receives the relay's push (a data message: `sealed` is the encrypted `notice`, which Google and the relay never read),
 * opens it with the `box` key and posts the notification. Nothing in the message is shown as sent: a payload that cannot
 * be opened, or that the `event` window refuses, posts nothing.
 *
 * The app builds without a Firebase project (`google-services.json` is added by the owner, see docs/development.md); this
 * service then simply never receives anything.
 */
class PushService : FirebaseMessagingService() {
    override fun onMessageReceived(message: RemoteMessage) {
        handle(applicationContext, message.data["sealed"])
    }

    override fun onNewToken(token: String) {
        // The app registers it with the relays (Dart listens for `token`). If the app is not running, `register` at the
        // next start fetches the current token again.
        PushChannel.active?.emitToken(token)
    }

    companion object {
        fun peerDir(context: Context) = File(context.noBackupFilesDir, "push")

        /**
         * The whole path of one push, apart from Firebase: open, decide, post. Returns whether anything was done. Anything
         * that goes wrong (keys not readable, a store file that does not parse, the library not loading) drops the push:
         * a notification service must never crash on one bad message.
         */
        fun handle(context: Context, sealed: String?, keys: KeyStore? = null): Boolean = try {
            val store = keys ?: KeyStore(context)
            val opened = NoticeHandler(PeerStore(peerDir(context))) { store.secret("box") }.open(sealed)
            val action = opened?.let { planFor(it, PushPrefs(context).hideDetails) }
            // The surfaces outside the app follow the notice, whether or not it is shown as a notification.
            opened?.let { dev.bobix.phone.surfaces.Surfaces.onNotice(context, it.computerId, it.notice.what, it.notice.reference) }
            if (action != null) NoticePoster(context).apply(action)
            action != null
        } catch (e: Exception) {
            false
        } catch (e: UnsatisfiedLinkError) {
            false
        }
    }
}
