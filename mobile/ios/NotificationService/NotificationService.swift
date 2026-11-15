import UserNotifications

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
    PushDelivered.removeSuperseded(shaped.supersedes, current: request.identifier) { [weak self] in
      self?.finish(shaped.content)
    }
  }

  override func serviceExtensionTimeWillExpire() {
    // Out of time: the notification as the relay made it (the generic text).
    if let original { finish(original) }
  }
}
