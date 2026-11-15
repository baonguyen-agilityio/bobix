package dev.bobix.phone.surfaces

import android.content.Context
import android.content.Intent
import dev.bobix.phone.push.PushPrefs
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

/**
 * The platform channel `dev.bobix.phone/surfaces`, for the screens outside the app.
 *
 * Dart to native: `writeList(json)` (the shared list, see [SurfaceList]), `writePet({name, png})` (a pet image the widgets
 * show), `clear` (nothing is paired any more), `initialOpen` -> `{target, computer, id, kind}` of the tap that started the
 * app, once, or null; calling it also tells native that Dart listens for `open`.
 *
 * Native to Dart: `open` with the same map: `target` is `home`, `start` or `request` (then `id` is the request, plan or
 * question to open). A tap is accepted only from this app's own surfaces (they carry the app's secret), and Dart must still
 * treat it as input.
 */
class SurfacesChannel(messenger: BinaryMessenger, private val context: Context) {
    private val channel = MethodChannel(messenger, NAME)
    private var initial: Map<String, String>? = null
    private var dartListening = false

    init {
        channel.setMethodCallHandler(::handle)
    }

    fun dispose() = channel.setMethodCallHandler(null)

    fun onIntent(intent: Intent?, started: Boolean) {
        if (intent?.action != Surfaces.ACTION_OPEN) return
        if (intent.getStringExtra(Surfaces.EXTRA_TOKEN) != PushPrefs(context).tapToken) return
        val open = mapOf(
            "target" to (intent.getStringExtra(Surfaces.EXTRA_TARGET) ?: "home"),
            "computer" to (intent.getStringExtra(Surfaces.EXTRA_COMPUTER) ?: ""),
            "id" to (intent.getStringExtra(Surfaces.EXTRA_ID) ?: ""),
            "kind" to (intent.getStringExtra(Surfaces.EXTRA_KIND) ?: ""),
        )
        if (started || !dartListening) initial = open else channel.invokeMethod("open", open)
    }

    private fun handle(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "writeList" -> {
                val ok = Surfaces.store(context).write(call.argument<String>("json") ?: "")
                result.success(ok)
                if (ok) Thread { Surfaces.refresh(context) }.start()
            }
            "writePet" -> result.success(Surfaces.store(context).writePet(call.argument<String>("name") ?: "", call.argument<ByteArray>("png") ?: ByteArray(0)))
            "clear" -> {
                Surfaces.store(context).clear()
                result.success(null)
                Thread { Surfaces.refresh(context) }.start()
            }
            "initialOpen" -> {
                dartListening = true
                result.success(initial)
                initial = null
            }
            else -> result.notImplemented()
        }
    }

    companion object {
        const val NAME = "dev.bobix.phone/surfaces"
    }
}
