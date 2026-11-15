package dev.bobix.phone.surfaces

import android.content.Context
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

/**
 * The platform channel `dev.bobix.phone/surfaces`, the same one `ios/Runner/SurfacesChannel.swift` answers, for the screens
 * outside the app (`lib/surfaces/surfaces_platform.dart` calls it).
 *
 * Dart to native: `capabilities` -> `{liveActivities: false}` (a Live Update is raised from the list here, not driven by Dart),
 * `writeList({json})` (the shared list, see [SurfaceList]), `writePet({design, color, state, png})`, `clearSurfaces` (nothing
 * is paired any more), and `syncActivity` -> `{active: false}`.
 *
 * A tap on a surface does not come through here: it reaches Dart as a tap of the push channel (see `PushChannel.onIntent`).
 */
class SurfacesChannel(messenger: BinaryMessenger, private val context: Context) {
    private val channel = MethodChannel(messenger, NAME)

    init {
        channel.setMethodCallHandler(::handle)
    }

    fun dispose() = channel.setMethodCallHandler(null)

    private fun handle(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "capabilities" -> result.success(mapOf("liveActivities" to false))
            "syncActivity" -> result.success(mapOf("active" to false))
            "writeList" -> {
                val ok = Surfaces.store(context).write(call.argument<String>("json") ?: "")
                result.success(ok)
                if (ok) Thread { Surfaces.refresh(context) }.start()
            }
            "writePet" -> {
                val name = "${call.argument<String>("design")}-${call.argument<Int>("color")}-${call.argument<String>("state")}.png"
                result.success(Surfaces.store(context).writePet(name, call.argument<ByteArray>("png") ?: ByteArray(0)))
            }
            "clearSurfaces" -> {
                Surfaces.store(context).clear()
                result.success(null)
                Thread { Surfaces.refresh(context) }.start()
            }
            else -> result.notImplemented()
        }
    }

    companion object {
        const val NAME = "dev.bobix.phone/surfaces"
    }
}
