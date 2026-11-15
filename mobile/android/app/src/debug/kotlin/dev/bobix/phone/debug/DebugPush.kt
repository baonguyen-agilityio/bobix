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
 */
class DebugPush : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val app = context.applicationContext
        when (intent.action) {
            "dev.bobix.phone.debug.SEED" -> {
                KeyStore(app).putSecret("box", hex(intent.getStringExtra("box")!!))
                PeerStore(PushService.peerDir(app)).set("desktop-1", hex(intent.getStringExtra("desktop")!!)) { NoticeHandler.freshWindow() }
                Log.i(TAG, "seeded")
            }
            "dev.bobix.phone.debug.DELIVER" -> Log.i(TAG, "delivered: ${PushService.handle(app, intent.getStringExtra("sealed"))}")
            "dev.bobix.phone.debug.HIDE" -> PushPrefs(app).hideDetails = intent.getBooleanExtra("value", false)
        }
    }

    private fun hex(s: String) = s.chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    companion object {
        const val TAG = "BobixDebugPush"
    }
}
