import UserNotifications
import WidgetKit

/// Opens the sealed notice of an alert push and writes its text. One decryption, no network, no signing key.
final class NotificationService: UNNotificationServiceExtension {
  private var handler: ((UNNotificationContent) -> Void)?
  private var original: UNNotificationContent?

  /// The system gives one chance to hand content back: whichever of the normal path and the time-out gets here first.
  private func finish(_ content: UNNotificationContent) {
    guard let h = handler else { return }
    handler = nil
    h(content)
  }

  override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
    handler = contentHandler
    original = request.content
    let shaped = PushOpener.live().shape(request.content)
    // The widgets and the Live Activity follow the notice: a request waits, or it was answered.
    if let n = shaped.notice {
      var list = SharedListStore.load()
      list.apply(kind: n.kind, ref: n.ref, computer: n.computer, title: n.title)
      if (try? SharedListStore.save(list)) != nil { WidgetCenter.shared.reloadAllTimelines() }
    }
    PushDelivered.removeSuperseded(shaped.supersedes, current: request.identifier) { [weak self] in
      self?.finish(shaped.content)
    }
  }

  override func serviceExtensionTimeWillExpire() {
    // Out of time: the notification as the relay made it (the generic text).
    if let original { finish(original) }
  }
}
