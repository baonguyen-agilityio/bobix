package dev.bobix.phone

import android.content.Intent
import android.view.WindowManager
import dev.bobix.phone.keys.SecureKeysChannel
import dev.bobix.phone.push.PushChannel
import dev.bobix.phone.surfaces.SurfacesChannel
import io.flutter.embedding.android.FlutterFragmentActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/**
 * A `FlutterFragmentActivity`, not a `FlutterActivity`: the biometric prompt that guards the signing key needs a
 * `FragmentActivity`. It hosts the platform channels: the keys, push, and `window` (the screens that show a request, plan
 * or question ask for `setSecure(true)`, so the recents screen and screenshots show nothing of them).
 */
class MainActivity : FlutterFragmentActivity() {
    private var push: PushChannel? = null
    private var surfaces: SurfacesChannel? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        val messenger = flutterEngine.dartExecutor.binaryMessenger
        SecureKeysChannel(messenger, this)
        push = PushChannel(messenger, this).also { it.onIntent(intent, started = true) }
        surfaces = SurfacesChannel(messenger, applicationContext).also { it.onIntent(intent, started = true) }
        MethodChannel(messenger, WINDOW_CHANNEL).setMethodCallHandler { call, result ->
            when (call.method) {
                "setSecure" -> {
                    if (call.argument<Boolean>("secure") == true) window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
                    else window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        push?.onIntent(intent, started = false)
        surfaces?.onIntent(intent, started = false)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (PushChannel.isPermissionRequest(requestCode)) push?.onPermissionResult()
    }

    override fun cleanUpFlutterEngine(flutterEngine: FlutterEngine) {
        push?.dispose()
        surfaces?.dispose()
        super.cleanUpFlutterEngine(flutterEngine)
    }

    companion object {
        const val WINDOW_CHANNEL = "dev.bobix.phone/window"
    }
}
