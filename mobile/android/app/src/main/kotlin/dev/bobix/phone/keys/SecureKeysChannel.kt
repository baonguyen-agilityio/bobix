package dev.bobix.phone.keys

import androidx.fragment.app.FragmentActivity
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

/**
 * The platform channel `dev.bobix.phone/secure_keys`: the Dart side of the key interface (see lib/keys). The same
 * methods and answers as the iOS channel, plus `strongBiometrics` and `signingKeyLevel`, which only Android needs.
 */
class SecureKeysChannel(messenger: BinaryMessenger, private val activity: FragmentActivity) {
    private var store: KeyStore? = null

    init {
        MethodChannel(messenger, NAME).setMethodCallHandler(::handle)
    }

    private fun handle(call: MethodCall, result: MethodChannel.Result) {
        if (call.method == "open") {
            try {
                store = KeyStore(activity.applicationContext, software = call.argument<Boolean>("software") ?: false)
                result.success(null)
            } catch (e: Exception) {
                result.error("open_failed", e.message, null)
            }
            return
        }
        val store = store ?: return result.error("not_open", "call open first", null)
        fun name() = call.argument<String>("name") ?: ""
        fun bytes(key: String) = call.argument<ByteArray>(key) ?: ByteArray(0)
        try {
            when (call.method) {
                "putSecret" -> { store.putSecret(name(), bytes("data")); result.success(null) }
                "secret" -> result.success(store.secret(name()))
                "deleteSecret" -> { store.deleteSecret(name()); result.success(null) }
                "createSigningKey" -> result.success(store.createSigningKey())
                "signingPublicKey" -> result.success(store.signingPublicKey())
                "deleteSigningKey" -> { store.deleteSigningKey(); result.success(null) }
                "eraseAll" -> { store.eraseAll(); result.success(null) }
                "strongBiometrics" -> result.success(store.strongBiometrics())
                "signingKeyLevel" -> result.success(store.signingKeyLevel())
                "authenticate" -> store.authenticate(activity, call.argument<String>("reason") ?: "") { result.success(it) }
                "sign" -> store.sign(activity, bytes("data"), call.argument<String>("reason") ?: "") { result.success(answer(it)) }
                else -> result.notImplemented()
            }
        } catch (e: KeyStore.KeyError) {
            result.error("key_error", e.message, null)
        } catch (e: java.security.GeneralSecurityException) {
            result.error("key_error", e.message, null)
        }
    }

    private fun answer(o: KeyStore.SignOutcome): Map<String, Any> = when (o) {
        is KeyStore.SignOutcome.Signed -> mapOf("result" to "signed", "signature" to o.signature)
        KeyStore.SignOutcome.Cancelled -> mapOf("result" to "cancelled")
        KeyStore.SignOutcome.LockedOut -> mapOf("result" to "lockedOut")
        KeyStore.SignOutcome.Unavailable -> mapOf("result" to "unavailable")
        KeyStore.SignOutcome.Invalidated -> mapOf("result" to "invalidated")
        is KeyStore.SignOutcome.Failed -> mapOf("result" to "failed", "message" to o.message)
    }

    companion object {
        const val NAME = "dev.bobix.phone/secure_keys"
    }
}
