package dev.bobix.phone.push

import java.util.Base64
import uniffi.bobix_phone_core.OpenNoticeException
import uniffi.bobix_phone_core.OpenedNotice
import uniffi.bobix_phone_core.newNoticeWindow
import uniffi.bobix_phone_core.openNotice
import uniffi.bobix_phone_core.openPacket

/** What to do with a notice that opened. Pure data: the Android side only carries it out. */
sealed interface NoticeAction {
    /** Post (or replace, by [tag]) a notification. A null [title] or [body] means the generic text. */
    class Show(
        val needsYou: Boolean,
        val tag: String,
        val title: String?,
        val body: String?,
        val computerId: String,
        val reference: String,
        val what: String,
    ) : NoticeAction

    class Remove(val tag: String) : NoticeAction
}

/** A notice and the computer it came from. */
class FromComputer(val computerId: String, val notice: OpenedNotice)

/** `waiting` replaces the earlier notification of its `ref`, `closed` removes it, `done` has its own. */
fun planFor(from: FromComputer, hideDetails: Boolean): NoticeAction? {
    val n = from.notice
    val c = from.computerId
    return when (n.what) {
        "waiting" -> NoticeAction.Show(true, "r:$c:${n.reference}", n.title.takeUnless { hideDetails }, n.body.takeUnless { hideDetails }, c, n.reference, n.what)
        "closed" -> NoticeAction.Remove("r:$c:${n.reference}")
        "done" -> NoticeAction.Show(false, "d:$c:${n.reference}", n.title.takeUnless { hideDetails }, n.body.takeUnless { hideDetails }, c, n.reference, n.what)
        else -> null
    }
}

/** What opening a packet through the shared window gave (the `openPacket` call of the push channel). */
sealed interface PacketResult {
    class Opened(val computerId: String, val stream: String, val counter: Long, val plaintext: ByteArray) : PacketResult

    /** `replayed`, `version`, `unreadable` or `noKey`. */
    class Refused(val why: String) : PacketResult
}

/**
 * Opens a pushed `sealed` payload, or any packet the app hands over, with this phone's `box` key against the computers it
 * is paired with. Anything it cannot open (not base64, not from a paired computer, altered, replayed, outside the `event`
 * window, not a notice) is dropped, and a dropped push leaves every window as it was.
 */
class NoticeHandler(private val peers: PeerStore, private val boxSecret: () -> ByteArray?) {
    fun open(sealed: String?): FromComputer? {
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
                    val opened = openNotice(secret, peer.boxPublic, packet, peer.state, peer.versionMin.toUByte(), peer.versionMax.toUByte())
                    peers.saveState(peer.id, opened.state)
                    return@locked FromComputer(peer.id, opened)
                } catch (e: OpenNoticeException) {
                    // Not this computer's (or refused): try the next one.
                }
            }
            null
        }
    }

    /** Opens any packet from a paired computer; what the extension (here: the push service) already took is `replayed`. */
    fun openAny(packet: ByteArray): PacketResult {
        val secret = boxSecret() ?: return PacketResult.Refused("noKey")
        return peers.locked {
            var why = "unreadable"
            for (peer in peers.all()) {
                try {
                    val o = openPacket(secret, peer.boxPublic, packet, peer.state, peer.versionMin.toUByte(), peer.versionMax.toUByte())
                    peers.saveState(peer.id, o.state)
                    return@locked PacketResult.Opened(peer.id, o.stream, o.counter.toLong(), o.plaintext)
                } catch (e: OpenNoticeException.Replayed) {
                    why = "replayed"
                } catch (e: OpenNoticeException.Version) {
                    if (why != "replayed") why = "version"
                } catch (e: OpenNoticeException) {
                    // Not this computer's.
                }
            }
            PacketResult.Refused(why)
        }
    }

    companion object {
        /** A push carries at most 4 KiB; anything much larger is not one of ours. */
        const val MAX_SEALED = 16 * 1024

        fun freshWindow(): String = newNoticeWindow()
    }
}
