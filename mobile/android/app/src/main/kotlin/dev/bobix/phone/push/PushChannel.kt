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
import dev.bobix.phone.keys.KeyStore
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import uniffi.bobix_phone_core.makeGrant

/**
 * The platform channel `dev.bobix.phone/push`, answering what `lib/push/push_platform.dart` (the `ChannelPushPlatform`
 * the iOS push work defines) calls: the same method names, argument names and answer maps as `ios/Runner/PushChannel.swift`.
 *
 * Dart to native:
 * - `permissionStatus` -> "notDetermined" | "denied" | "authorized"; `requestPermission` -> whether allowed (the system
 *   dialog on Android 13 and later, none before; registers with FCM when allowed); `openSettings`.
 * - `register` fetches the FCM token and hands it over as a `token` call to Dart; `token` -> the current token, or null
 *   when this build has no Firebase project (`google-services.json` missing) or none has been handed over yet.
 * - `environment` -> always "production" (APNs has sandbox and production; FCM has one).
 * - `grant({token, relayId, expiresMs})` -> the signed grant; the `auth` seed stays on this side.
 * - `initialTap` -> {computer, ref, kind} of the notification tap that started the app, once, or null. Calling it also
 *   tells native that Dart now listens for `tap`; a tap that arrives before that waits for this call.
 * - `setComputer({id, boxPublic, versionMin, versionMax})`, `removeComputer({id})`, `openPacket({packet})` ->
 *   {computer, stream, counter, plaintext} or {error: replayed|version|unreadable|noKey}: the receive window shared with
 *   the push service, which is a separate entry point into this app.
 * - `hideDetails` and `setHideDetails({hide})`: the switch the push service reads without the app being open.
 * - `clearDelivered`: cancels every notification of this app that is still shown.
 *
 * Native to Dart: `token` (a String), `tokenError` (a String) and `tap` ({computer, ref, kind}).
 *
 * A tap is accepted only from this app's own notifications (they carry a secret in the intent); see [onIntent].
 */
class PushChannel(messenger: BinaryMessenger, private val activity: FragmentActivity) {
    private val channel = MethodChannel(messenger, NAME)
    private val prefs = PushPrefs(activity.applicationContext)
    private val peers = PeerStore(PushService.peerDir(activity.applicationContext))
    private var initialTap: Map<String, String>? = null
    /** Dart's first `initialTap` call is also its "I am listening" signal (as on iOS): until then no tap goes over the channel. */
    private var dartListening = false
    private var permissionResult: MethodChannel.Result? = null
    private var lastToken: String? = null

    init {
        channel.setMethodCallHandler(::handle)
        active = this
    }

    fun dispose() {
        if (active === this) active = null
        channel.setMethodCallHandler(null)
    }

    /**
     * The activity was started (or brought forward) with [intent]. Only a tap made by this app's own notification counts:
     * [MainActivity] is exported (it is the launcher), so another app can send it an `OPEN_NOTICE` intent with any
     * extras, and those carry no [NoticePoster.EXTRA_TOKEN] that matches.
     */
    fun onIntent(intent: Intent?, started: Boolean) {
        if (intent?.action != NoticePoster.ACTION_OPEN_NOTICE) return
        if (intent.getStringExtra(NoticePoster.EXTRA_TOKEN) != prefs.tapToken) return
        val tap = mapOf(
            "computer" to (intent.getStringExtra(NoticePoster.EXTRA_COMPUTER) ?: ""),
            "ref" to (intent.getStringExtra(NoticePoster.EXTRA_REF) ?: ""),
            "kind" to (intent.getStringExtra(NoticePoster.EXTRA_KIND) ?: ""),
        )
        if (started || !dartListening) initialTap = tap else channel.invokeMethod("tap", tap)
    }

    /** A token the system handed over (first time, or changed). */
    fun emitToken(token: String) {
        lastToken = token
        activity.runOnUiThread { channel.invokeMethod("token", token) }
    }

    fun onPermissionResult() {
        val pending = permissionResult ?: return
        permissionResult = null
        val granted = permission() == "authorized"
        if (granted) fetchToken()
        pending.success(granted)
    }

    private fun permission(): String {
        val enabled = NotificationManagerCompat.from(activity).areNotificationsEnabled()
        val granted = enabled && (Build.VERSION.SDK_INT < 33 || ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED)
        return when {
            granted -> "authorized"
            prefs.asked || Build.VERSION.SDK_INT < 33 -> "denied"
            else -> "notDetermined"
        }
    }

    private fun firebaseReady() = FirebaseApp.getApps(activity).isNotEmpty()

    /** Asks FCM for the token and hands it to Dart. No Firebase project in this build: nothing happens. */
    private fun fetchToken() {
        if (!firebaseReady()) return
        FirebaseMessaging.getInstance().token.addOnCompleteListener { t ->
            if (t.isSuccessful) emitToken(t.result) else channel.invokeMethod("tokenError", t.exception?.message ?: "token failed")
        }
    }

    private fun handle(call: MethodCall, result: MethodChannel.Result) {
        try {
            when (call.method) {
                "permissionStatus" -> result.success(permission())
                "requestPermission" -> when {
                    permission() == "authorized" -> { fetchToken(); result.success(true) }
                    Build.VERSION.SDK_INT < 33 || permissionResult != null -> result.success(false)
                    else -> {
                        prefs.asked = true
                        permissionResult = result
                        activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), PERMISSION_CODE)
                    }
                }
                "register" -> { fetchToken(); result.success(null) }
                // The pairing ended: no notification of this app stays in the shade with a session name in it.
                "clearDelivered" -> { NotificationManagerCompat.from(activity).cancelAll(); result.success(null) }
                "token" -> result.success(lastToken)
                "environment" -> result.success("production")
                "grant" -> grant(call, result)
                "initialTap" -> { dartListening = true; result.success(initialTap); initialTap = null }
                "openSettings" -> {
                    activity.startActivity(
                        Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName),
                    )
                    result.success(null)
                }
                "setComputer" -> {
                    val box = call.argument<ByteArray>("boxPublic") ?: ByteArray(0)
                    val min = call.argument<Int>("versionMin") ?: 1
                    val max = call.argument<Int>("versionMax") ?: 1
                    if (box.size != 32 || min < 1 || min > max || max > 255) return result.error("push_error", "a box public key is 32 bytes and 1 <= versionMin <= versionMax", null)
                    peers.set(call.argument<String>("id") ?: "", box, min, max) { NoticeHandler.freshWindow() }
                    result.success(null)
                }
                "removeComputer" -> { peers.remove(call.argument<String>("id") ?: ""); result.success(null) }
                "openPacket" -> openPacket(call, result)
                "hideDetails" -> result.success(prefs.hideDetails)
                "setHideDetails" -> { prefs.hideDetails = call.argument<Boolean>("hide") ?: false; result.success(null) }
                else -> result.notImplemented()
            }
        } catch (e: IllegalArgumentException) {
            result.error("push_error", e.message, null)
        }
    }

    private fun keys() = KeyStore(activity.applicationContext)

    private fun grant(call: MethodCall, result: MethodChannel.Result) {
        try {
            val store = keys()
            val auth = store.secret("auth")
            val box = store.secret("box")
            if (auth == null || box == null) return result.error("no_key", "the device keys are not there", null)
            val expires = (call.argument<Number>("expiresMs") ?: 0).toLong()
            if (expires <= 0) return result.error("push_error", "expiresMs must be a positive time in milliseconds", null)
            result.success(makeGrant(auth, box, call.argument<String>("token") ?: "", call.argument<String>("relayId") ?: "", expires.toULong()))
        } catch (e: Exception) {
            result.error("push_error", e.message, null)
        }
    }

    private fun openPacket(call: MethodCall, result: MethodChannel.Result) {
        val box = try {
            keys().secret("box")
        } catch (e: Exception) {
            null
        } ?: return result.success(mapOf("error" to "noKey"))
        val packet = call.argument<ByteArray>("packet") ?: ByteArray(0)
        result.success(
            when (val r = NoticeHandler(peers) { box }.openAny(packet)) {
                is PacketResult.Opened -> mapOf("computer" to r.computerId, "stream" to r.stream, "counter" to r.counter, "plaintext" to r.plaintext)
                is PacketResult.Refused -> mapOf("error" to r.why)
            },
        )
    }

    companion object {
        const val NAME = "dev.bobix.phone/push"
        private const val PERMISSION_CODE = 4501

        /** For the service, which can be asked to run while the app is open. */
        @Volatile var active: PushChannel? = null

        fun isPermissionRequest(code: Int) = code == PERMISSION_CODE
    }
}
