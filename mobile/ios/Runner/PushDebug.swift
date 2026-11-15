#if DEBUG
import Foundation
import UserNotifications

/// Debug builds only. The simulator never starts a notification service extension for `simctl push`, so the same opener
/// the extension runs (`PushOpener`) is driven from launch arguments instead:
///
///   -bobix-seed-push <fixtures.json>      stores the fixture's `box` key and paired computer, as pairing will
///   -bobix-simulate-push <push.json>      `{"identifier": "...", "sealed": "..."}`: opens it and shows the result as a
///                                         notification, the way the extension would, replacing earlier ones of its request
///
/// Compiled out of every release build.
enum PushDebug {
  static func run() {
    let a = ProcessInfo.processInfo.arguments
    if let i = a.firstIndex(of: "-bobix-seed-push"), i + 1 < a.count { seed(a[i + 1]) }
    if let i = a.firstIndex(of: "-bobix-simulate-push"), i + 1 < a.count { simulate(a[i + 1]) }
  }

  private static func json(_ path: String) -> [String: Any]? {
    (try? Data(contentsOf: URL(fileURLWithPath: path))).flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
  }

  private static func data(urlSafeBase64 s: String) -> Data? {
    var t = s.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
    while t.count % 4 != 0 { t += "=" }
    return Data(base64Encoded: t)
  }

  private static func data(hex: String) -> Data {
    var d = Data()
    var i = hex.startIndex
    while i < hex.endIndex, let j = hex.index(i, offsetBy: 2, limitedBy: hex.endIndex) {
      if let b = UInt8(hex[i..<j], radix: 16) { d.append(b) }
      i = j
    }
    return d
  }

  private static func seed(_ path: String) {
    guard let f = json(path), let secret = f["phoneBoxSecretHex"] as? String, let c = f["computer"] as? [String: Any],
      let id = c["id"] as? String, let pub = (c["boxPublicB64"] as? String).flatMap({ data(urlSafeBase64: $0) }),
      let store = PushPaths.storePath(), let keys = try? KeyStore()
    else { return NSLog("bobix seed: the fixture or the app group is not usable") }
    do {
      try keys.putSecret("box", data(hex: secret))
      try storeRemoveComputer(storePath: store, computerId: id)  // a seed starts the receive window from nothing
      try storeSetComputer(storePath: store, computerId: id, peerBoxPublic: pub, versionMin: 1, versionMax: 1)
      if let hide = f["hideDetails"] as? Bool { try storeSetHideDetails(storePath: store, hide: hide) }
      NSLog("bobix seed: ok")
    } catch { NSLog("bobix seed failed: \(error)") }
  }

  private static func simulate(_ path: String) {
    guard let p = json(path), let sealed = p["sealed"] as? String else { return NSLog("bobix simulate: no sealed payload") }
    let identifier = p["identifier"] as? String ?? UUID().uuidString
    let strings = PushStrings.bundled()
    let original = UNMutableNotificationContent()
    original.title = strings.genericTitle  // what iOS makes of the relay's loc-keys
    original.body = strings.genericBody
    original.sound = .default
    original.userInfo = ["sealed": sealed]
    let shaped = PushOpener.live().shape(original)
    NSLog("bobix simulate: title=\(shaped.content.title) | body=\(shaped.content.body) | level=\(shaped.content.interruptionLevel.rawValue)")
    UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in
      PushDelivered.removeSuperseded(shaped.supersedes, current: identifier) {
        let request = UNNotificationRequest(
          identifier: identifier, content: shaped.content, trigger: UNTimeIntervalNotificationTrigger(timeInterval: 1, repeats: false))
        UNUserNotificationCenter.current().add(request) { e in NSLog("bobix simulate: scheduled \(e.map { "\($0)" } ?? "ok")") }
      }
    }
  }
}
#endif
