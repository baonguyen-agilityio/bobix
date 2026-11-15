package dev.bobix.phone.push

import java.util.Base64
import uniffi.bobix_phone_core.OpenNoticeException
import uniffi.bobix_phone_core.OpenedNotice
import uniffi.bobix_phone_core.newNoticeWindow
import uniffi.bobix_phone_core.openNotice

/** What to do with a notice that opened. Pure data: the Android side only carries it out. */
sealed interface NoticeAction {
    /** Post (or replace, by [tag]) a notification. A null [title] or [body] means the generic text. */
    class Show(
        val needsYou: Boolean,
        val tag: String,
        val title: String?,
        val body: String?,
        val reference: String,
        val what: String,
        val session: String,
    ) : NoticeAction

    class Remove(val tag: String) : NoticeAction
}

/** `waiting` replaces the earlier notification of its `ref`, `closed` removes it, `done` has its own. */
fun planFor(n: OpenedNotice, hideDetails: Boolean): NoticeAction? = when (n.what) {
    "waiting" -> NoticeAction.Show(true, "r:${n.reference}", n.title.takeUnless { hideDetails }, n.body.takeUnless { hideDetails }, n.reference, n.what, n.session)
    "closed" -> NoticeAction.Remove("r:${n.reference}")
    "done" -> NoticeAction.Show(false, "d:${n.reference}", n.title.takeUnless { hideDetails }, n.body.takeUnless { hideDetails }, n.reference, n.what, n.session)
    else -> null
}

/**
 * Opens a pushed `sealed` payload with this phone's `box` key against the computers it is paired with. Anything it cannot
 * open (not base64, not from a paired computer, altered, replayed, outside the `event` window, not a notice) is dropped,
 * and a dropped push leaves every window as it was.
 */
class NoticeHandler(private val peers: PeerStore, private val boxSecret: () -> ByteArray?) {
    fun open(sealed: String?): OpenedNotice? {
        if (sealed.isNullOrEmpty() || sealed.length > MAX_SEALED) return null
        val packet = try {
            Base64.getUrlDecoder().decode(sealed)
        } catch (e: IllegalArgumentException) {
            return null
        }
        val secret = boxSecret() ?: return null
        return peers.locked {
            for (peer in peers.all()) {
                try {
                    val opened = openNotice(secret, peer.boxPublic, packet, peer.state)
                    peers.saveState(peer.id, opened.state)
                    return@locked opened
                } catch (e: OpenNoticeException) {
                    // Not this computer's (or refused): try the next one.
                }
            }
            null
        }
    }

    companion object {
        /** A push carries at most 4 KiB; anything much larger is not one of ours. */
        const val MAX_SEALED = 16 * 1024

        fun freshWindow(): String = newNoticeWindow()
    }
}
