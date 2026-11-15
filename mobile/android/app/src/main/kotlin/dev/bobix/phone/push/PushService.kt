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
        // The app is not necessarily running: it registers the new token the next time it opens (or now, if it is).
        PushPrefs(applicationContext).tokenChanged = true
        PushChannel.active?.tokenRefreshed()
    }

    companion object {
        fun peerDir(context: Context) = File(context.noBackupFilesDir, "push")

        /** The whole path of one push, apart from Firebase: open, decide, post. Returns whether anything was done. */
        fun handle(context: Context, sealed: String?, keys: KeyStore? = null): Boolean {
            val store = keys ?: try {
                KeyStore(context)
            } catch (e: KeyStore.KeyError) {
                return false
            }
            val opened = NoticeHandler(PeerStore(peerDir(context))) { store.secret("box") }.open(sealed) ?: return false
            val action = planFor(opened, PushPrefs(context).hideDetails) ?: return false
            NoticePoster(context).apply(action)
            return true
        }
    }
}
