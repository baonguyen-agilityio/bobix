package dev.bobix.phone.push

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

/**
 * The platform channel `dev.bobix.phone/push`: the Android half of what the Dart side needs from the OS for push.
 *
 * Dart to native:
 * - `permission` -> "granted" | "denied"; `requestPermission` -> the same, after the system dialog (Android 13 and later;
 *   earlier versions have no dialog); `openSettings` opens this app's notification settings.
 * - `token` -> the FCM token, or null when this build has no Firebase project (`google-services.json` is missing).
 *   Firebase starts only when this is first called, which is after pairing; it never starts at launch.
 * - `deleteToken` forgets the token at Google (on disconnect from the last computer).
 * - `setHideDetails(bool)` and `hideDetails`: the "Hide details" switch the push service reads without the app open.
 * - `setPeer(id, boxPublic)` and `removePeer(id)`: the computers the push service may open notices from.
 * - `launchTap` -> the notification tap that started the app ({ref, what, session}), once, or null.
 * - `tokenChanged` -> whether the OS changed the token while the app was not running; reading it clears it.
 *
 * Native to Dart: `tap` ({ref, what, session}) when a notification is tapped while the app runs, and `tokenRefreshed`.
 */
class PushChannel(messenger: BinaryMessenger, private val activity: FragmentActivity) {
    private val channel = MethodChannel(messenger, NAME)
    private val prefs = PushPrefs(activity.applicationContext)
    private val peers = PeerStore(PushService.peerDir(activity.applicationContext))
    private var launchTap: Map<String, String>? = null
    private var permissionResult: MethodChannel.Result? = null

    init {
        channel.setMethodCallHandler(::handle)
        active = this
    }

    fun dispose() {
        if (active === this) active = null
        channel.setMethodCallHandler(null)
    }

    /** The activity was started (or brought forward) by a tap on a notification. */
    fun onIntent(intent: Intent?, started: Boolean) {
        if (intent?.action != NoticePoster.ACTION_OPEN_NOTICE) return
        val tap = mapOf(
            "ref" to (intent.getStringExtra(NoticePoster.EXTRA_REF) ?: ""),
            "what" to (intent.getStringExtra(NoticePoster.EXTRA_WHAT) ?: ""),
            "session" to (intent.getStringExtra(NoticePoster.EXTRA_SESSION) ?: ""),
        )
        if (started) launchTap = tap else channel.invokeMethod("tap", tap)
    }

    fun tokenRefreshed() {
        activity.runOnUiThread { channel.invokeMethod("tokenRefreshed", null) }
    }

    fun onPermissionResult() {
        permissionResult?.success(permission())
        permissionResult = null
    }

    private fun permission(): String {
        val ok = NotificationManagerCompat.from(activity).areNotificationsEnabled() &&
            (Build.VERSION.SDK_INT < 33 || ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED)
        return if (ok) "granted" else "denied"
    }

    private fun handle(call: MethodCall, result: MethodChannel.Result) {
        try {
            when (call.method) {
                "permission" -> result.success(permission())
                "requestPermission" -> when {
                    permission() == "granted" -> result.success("granted")
                    Build.VERSION.SDK_INT < 33 || permissionResult != null -> result.success(permission())
                    else -> {
                        permissionResult = result
                        activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), PERMISSION_CODE)
                    }
                }
                "openSettings" -> {
                    activity.startActivity(
                        Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName),
                    )
                    result.success(null)
                }
                "token" -> token(result)
                "deleteToken" -> {
                    if (FirebaseApp.getApps(activity).isEmpty()) result.success(null)
                    else FirebaseMessaging.getInstance().deleteToken().addOnCompleteListener { result.success(null) }
                }
                "setHideDetails" -> { prefs.hideDetails = call.argument<Boolean>("value") ?: false; result.success(null) }
                "hideDetails" -> result.success(prefs.hideDetails)
                "setPeer" -> {
                    val box = call.argument<ByteArray>("boxPublic") ?: ByteArray(0)
                    if (box.size != 32) return result.error("bad_key", "a box public key is 32 bytes", null)
                    peers.set(call.argument<String>("id") ?: "", box) { NoticeHandler.freshWindow() }
                    result.success(null)
                }
                "removePeer" -> { peers.remove(call.argument<String>("id") ?: ""); result.success(null) }
                "launchTap" -> { result.success(launchTap); launchTap = null }
                "tokenChanged" -> { result.success(prefs.tokenChanged); prefs.tokenChanged = false }
                else -> result.notImplemented()
            }
        } catch (e: IllegalArgumentException) {
            result.error("bad_argument", e.message, null)
        }
    }

    private fun token(result: MethodChannel.Result) {
        if (FirebaseApp.getApps(activity).isEmpty()) return result.success(null)
        FirebaseMessaging.getInstance().token.addOnCompleteListener { t ->
            if (t.isSuccessful) result.success(t.result) else result.error("token_failed", t.exception?.message, null)
        }
    }

    companion object {
        const val NAME = "dev.bobix.phone/push"
        private const val PERMISSION_CODE = 4501

        /** For the service, which can be asked to run while the app is open. */
        @Volatile var active: PushChannel? = null

        fun isPermissionRequest(code: Int) = code == PERMISSION_CODE
    }
}
