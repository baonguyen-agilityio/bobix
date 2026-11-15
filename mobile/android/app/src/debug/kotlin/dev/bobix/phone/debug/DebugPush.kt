package dev.bobix.phone.debug

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import dev.bobix.phone.keys.KeyStore
import dev.bobix.phone.push.NoticeHandler
import dev.bobix.phone.push.PeerStore
import dev.bobix.phone.push.PushPrefs
import dev.bobix.phone.push.PushService

/**
 * Debug builds only (this file is in the `debug` source set): lets `adb` stand in for Firebase on an emulator that has no
 * Firebase project. Release builds do not contain it.
 *
 *   adb shell am broadcast -n dev.bobix.phone/.debug.DebugPush -a dev.bobix.phone.debug.SEED --es box 04..04 --es desktop ce8d..
 *   adb shell am broadcast -n dev.bobix.phone/.debug.DebugPush -a dev.bobix.phone.debug.DELIVER --es sealed <base64url>
 *   adb shell am broadcast -n dev.bobix.phone/.debug.DebugPush -a dev.bobix.phone.debug.HIDE --ez value true
 *   adb shell am broadcast -n dev.bobix.phone/.debug.DebugPush -a dev.bobix.phone.debug.SURFACES --es list '{...}'
 *   adb shell am start -n dev.bobix.phone/.debug.WidgetPreview --es size small   (hosts the real widget for a screenshot)
 */
class DebugPush : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val app = context.applicationContext
        when (intent.action) {
            "dev.bobix.phone.debug.SEED" -> {
                KeyStore(app).putSecret("box", hex(intent.getStringExtra("box")!!))
                PeerStore(PushService.peerDir(app)).set("desktop-1", hex(intent.getStringExtra("desktop")!!), 1, 1) { NoticeHandler.freshWindow() }
                Log.i(TAG, "seeded")
            }
            "dev.bobix.phone.debug.DELIVER" -> Log.i(TAG, "delivered: ${PushService.handle(app, intent.getStringExtra("sealed"))}")
            // The surfaces outside the app, without a computer: `--es list '<json>'` (or `--es list64 <base64 of it>`, which survives the shell's quote removal) writes the shared list (see SurfaceList),
            // `--es pet <file name> --es png <base64>` a pet image, `--ez clear true` empties it. Then everything redraws.
            "dev.bobix.phone.debug.SURFACES" -> {
                val store = dev.bobix.phone.surfaces.Surfaces.store(app)
                if (intent.getBooleanExtra("clear", false)) store.clear()
                intent.getStringExtra("pet")?.let { store.writePet(it, android.util.Base64.decode(intent.getStringExtra("png"), android.util.Base64.DEFAULT)) }
                (intent.getStringExtra("list") ?: intent.getStringExtra("list64")?.let { String(android.util.Base64.decode(it, android.util.Base64.DEFAULT)) })
                    ?.let { Log.i(TAG, "list written: ${store.write(it)}") }
                val pending = goAsync()
                Thread { try { dev.bobix.phone.surfaces.Surfaces.refresh(app) } finally { pending.finish() } }.start()
            }
            "dev.bobix.phone.debug.HIDE" -> PushPrefs(app).hideDetails = intent.getBooleanExtra("value", false)
        }
    }

    private fun hex(s: String) = s.chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    companion object {
        const val TAG = "BobixDebugPush"
    }
}
