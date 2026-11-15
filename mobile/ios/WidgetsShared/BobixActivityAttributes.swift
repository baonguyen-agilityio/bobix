import ActivityKit
import Foundation

/// The Live Activity. A push from the relay (`liveActivity`) can only set `content-state`, and whatever the relay puts
/// there is not trusted: it holds no key and could write any text. So the state carries no name and no computer: it holds
/// the sealed content the computer made (opened with the push core) and two numbers the app sets itself. What is drawn
/// is the sealed content if it opens, otherwise the shared list that only the app and the notification extension write.
struct BobixActivityAttributes: ActivityAttributes {
  struct ContentState: Codable, Hashable {
    /// The `activity` packet the computer sealed for this phone (URL-safe base64), as the push carries it.
    var sealed: String?
    var waiting: Int
    /// When the app last updated the activity, in unix milliseconds: the stale date is this plus 15 minutes.
    var written: Int64

    init(sealed: String? = nil, waiting: Int = 0, written: Int64 = 0) {
      self.sealed = sealed
      self.waiting = waiting
      self.written = written
    }

    // A push carries only `sealed`: every other key may be missing.
    init(from decoder: Decoder) throws {
      let c = try decoder.container(keyedBy: CodingKeys.self)
      sealed = try c.decodeIfPresent(String.self, forKey: .sealed)
      waiting = try c.decodeIfPresent(Int.self, forKey: .waiting) ?? 0
      written = try c.decodeIfPresent(Int64.self, forKey: .written) ?? 0
    }
  }

  var computerId: String
}

/// What the computer seals into an `activity` packet: counts and the redacted names, nothing else.
struct ActivityBody: Codable, Equatable {
  var waiting: Int
  var names: [String]
  var written: Int64
  var computerName: String
  var computerState: String
}

extension BobixActivityAttributes.ContentState {
  /// The content to draw. A sealed blob is opened through the push core (the `activity` stream, where a repeated counter
  /// opens again because the system may draw the same content more than once). Without a sealed blob the shared list is
  /// drawn. A sealed blob that does not open is not drawn as anything but "nothing needs you": no relay-supplied text.
  func resolved(storePath: String?, boxSecret: Data?, file: SharedList, now: Date = Date()) -> (list: SharedList, fromSealed: Bool) {
    guard let sealed else { return (file, false) }
    if let storePath, let boxSecret, let packet = Data(urlSafeBase64: sealed),
      let opened = try? openPacket(storePath: storePath, boxSecret: boxSecret, packet: packet), opened.stream == "activity",
      let body = try? JSONDecoder().decode(ActivityBody.self, from: opened.plaintext)
    {
      return (body.list, true)
    }
    let none = SharedList(written: Int64(now.timeIntervalSince1970 * 1000), computerName: "", computerState: "online", todo: 0, canStart: false, rows: [])
    return (none, false)
  }
}

extension ActivityBody {
  /// The list a surface draws: one waiting row per name, and nameless rows for the rest of the count.
  var list: SharedList {
    var rows = names.enumerated().map { SharedList.Row(id: "a\($0.offset)", name: $0.element, project: "", state: "waiting") }
    while rows.count < waiting { rows.append(SharedList.Row(id: "a\(rows.count)", name: "", project: "", state: "waiting")) }
    return SharedList(written: written, computerName: computerName, computerState: computerState, todo: 0, canStart: false, rows: rows)
  }
}

extension Data {
  /// The transport encoding of a sealed packet: URL-safe base64 without padding.
  init?(urlSafeBase64 s: String) {
    var t = s.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
    while t.count % 4 != 0 { t += "=" }
    self.init(base64Encoded: t)
  }
}
