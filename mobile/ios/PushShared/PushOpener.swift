import Foundation
import Security
import UserNotifications

/// What the app and its notification service extension share about push: where the shared file is, how `box` is read,
/// and how a sealed notice becomes the text of a notification. The extension does one decryption and no network call.

enum PushPaths {
  static var appGroup: String? {
    let name = Bundle.main.object(forInfoDictionaryKey: "BobixAppGroup") as? String
    return name?.isEmpty == false ? name : nil
  }

  /// The file that holds the receive state of every paired computer and the "Hide details" switch, in the App Group
  /// container so the app and the extension see the same one. Out of backups.
  static func storePath(group: String? = appGroup) -> String? {
    guard let group, let base = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) else { return nil }
    var dir = base.appendingPathComponent("push", isDirectory: true)
    try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    try? dir.setResourceValues(values)
    return dir.appendingPathComponent("push.json").path
  }
}

enum PushBoxKey {
  static let service = "dev.bobix.phone.keys"

  /// Reads `box` from the shared keychain group. Read only: `KeyStore` (the app's) erases every key when its install
  /// marker is missing, and the extension has no such marker, so it must never be constructed here.
  static func read(group: String? = Bundle.main.object(forInfoDictionaryKey: "BobixKeychainGroup") as? String) -> Data? {
    var query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: "box",
      kSecAttrSynchronizable as String: false,
      kSecReturnData as String: true,
      kSecMatchLimit as String: kSecMatchLimitOne,
    ]
    if let group, !group.isEmpty { query[kSecAttrAccessGroup as String] = group }
    var out: CFTypeRef?
    return SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess ? out as? Data : nil
  }
}

/// The texts the extension writes itself, from its own Localizable.strings (generated from `phone.push.*`).
struct PushStrings {
  var genericTitle: String
  var genericBody: String
  var closed: String
  var doneTitle: String
  var doneBody: String
  var appName: String

  static func bundled(_ bundle: Bundle = .main) -> PushStrings {
    func t(_ key: String) -> String { NSLocalizedString(key, bundle: bundle, comment: "") }
    return PushStrings(
      genericTitle: t("bobix.push.title"), genericBody: t("bobix.push.body"), closed: t("bobix.push.closed"),
      doneTitle: t("bobix.push.done.title"), doneBody: t("bobix.push.done.body"), appName: "Bobix")
  }
}

/// What a tap needs to open the right screen: the computer, the request (the notice's `ref`) and the kind.
enum PushRoute {
  static let key = "bobix"

  static func userInfo(computer: String, ref: String, kind: String) -> [String: Any] {
    [key: ["computer": computer, "ref": ref, "kind": kind]]
  }

  static func parse(_ userInfo: [AnyHashable: Any]) -> (computer: String, ref: String, kind: String)? {
    guard let d = userInfo[key] as? [String: Any], let c = d["computer"] as? String, let r = d["ref"] as? String, let k = d["kind"] as? String
    else { return nil }
    return (c, r, k)
  }
}

/// The notification the extension hands back, and which shown notifications it replaces.
struct PushShaped {
  let content: UNNotificationContent
  /// Set for `waiting` and `closed`: an earlier notification of the same computer and request is removed.
  let supersedes: (computer: String, ref: String)?
}

struct PushOpener {
  /// The shared file, or nil where the app has no App Group (the notice stays unopened).
  let storePath: String?
  let boxSecret: Data?
  let strings: PushStrings

  static func live() -> PushOpener {
    PushOpener(storePath: PushPaths.storePath(), boxSecret: PushBoxKey.read(), strings: .bundled())
  }

  /// A route is only ever written by this opener, from a notice that opened. A route that arrived in a push the opener
  /// did not open came from the sender of that push, not from the computer, so it is dropped.
  static func withoutRoute(_ content: UNNotificationContent) -> UNNotificationContent {
    guard content.userInfo[PushRoute.key] != nil, let out = content.mutableCopy() as? UNMutableNotificationContent else { return content }
    var info = content.userInfo
    info.removeValue(forKey: PushRoute.key)
    out.userInfo = info
    return out
  }

  /// Opens the sealed notice of `content` and writes its text. Anything it cannot open leaves the content as the relay
  /// made it, which already carries the generic text through its loc-keys; the receive state is never touched then.
  func shape(_ content: UNNotificationContent) -> PushShaped {
    guard let sealed = content.userInfo["sealed"] as? String, let storePath, let boxSecret,
      let out = content.mutableCopy() as? UNMutableNotificationContent
    else { return PushShaped(content: PushOpener.withoutRoute(content), supersedes: nil) }

    let notice: OpenedNotice
    do {
      notice = try openNotice(storePath: storePath, boxSecret: boxSecret, sealed: sealed)
    } catch PushError.Replayed {
      // A copy the app already took, or one older than the window: the generic text, quietly, and nothing else.
      out.title = strings.genericTitle
      out.body = strings.genericBody
      out.sound = nil
      out.interruptionLevel = .passive
      // As harmless as iOS allows: it cannot drop an alert push without Apple's filtering entitlement, so the copy
      // sinks to the bottom of the summary and says nothing.
      out.relevanceScore = 0
      out.userInfo = [:]
      return PushShaped(content: out, supersedes: nil)
    } catch {
      return PushShaped(content: PushOpener.withoutRoute(content), supersedes: nil)
    }

    // A failure to read the switch shows the less revealing text.
    let hide = (try? storeHideDetails(storePath: storePath)) ?? true
    out.userInfo = PushRoute.userInfo(computer: notice.computerId, ref: notice.reference, kind: notice.what)
    switch notice.what {
    case "closed":
      out.title = hide ? strings.appName : notice.title
      out.body = strings.closed
      out.sound = nil
      out.interruptionLevel = .passive
    case "done":
      out.title = hide ? strings.doneTitle : notice.title
      out.body = hide ? strings.doneBody : notice.body
    default:
      out.title = hide ? strings.genericTitle : notice.title
      out.body = hide ? strings.genericBody : notice.body
    }
    let supersedes = notice.what == "done" ? nil : (computer: notice.computerId, ref: notice.reference)
    return PushShaped(content: out, supersedes: supersedes)
  }
}

enum PushDelivered {
  /// Removes the notifications already shown for the same request (but not `current`, which the system is about to
  /// show): a second `waiting` for one `ref` leaves one notification, and a `closed` takes the place of its `waiting`
  /// even when the relay's collapse id did not already do so.
  static func removeSuperseded(
    _ s: (computer: String, ref: String)?, current: String, center: UNUserNotificationCenter = .current(),
    then done: @escaping () -> Void
  ) {
    guard let s else { return done() }
    center.getDeliveredNotifications { list in
      let ids = list.filter { n in
        guard n.request.identifier != current, let r = PushRoute.parse(n.request.content.userInfo) else { return false }
        return r.computer == s.computer && r.ref == s.ref
      }.map(\.request.identifier)
      if !ids.isEmpty { center.removeDeliveredNotifications(withIdentifiers: ids) }
      done()
    }
  }
}
