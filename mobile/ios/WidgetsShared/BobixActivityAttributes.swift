import ActivityKit
import Foundation

/// The Live Activity. A push from the relay (`liveActivity`) can only set `content-state`, and the relay puts the sealed
/// content in `sealed`; the app, which starts the activity when it opens, fills the counts itself. Both are drawn the
/// same way: `sealed` wins when it opens.
struct BobixActivityAttributes: ActivityAttributes {
  struct ContentState: Codable, Hashable {
    /// The `activity` packet the computer sealed for this phone (URL-safe base64), as the push carries it.
    var sealed: String?
    var waiting: Int
    var names: [String]
    var written: Int64
    var computerName: String
    var computerState: String

    init(sealed: String? = nil, waiting: Int = 0, names: [String] = [], written: Int64 = 0, computerName: String = "", computerState: String = "online") {
      self.sealed = sealed
      self.waiting = waiting
      self.names = names
      self.written = written
      self.computerName = computerName
      self.computerState = computerState
    }

    // A push carries only `sealed`: every other key may be missing.
    init(from decoder: Decoder) throws {
      let c = try decoder.container(keyedBy: CodingKeys.self)
      sealed = try c.decodeIfPresent(String.self, forKey: .sealed)
      waiting = try c.decodeIfPresent(Int.self, forKey: .waiting) ?? 0
      names = try c.decodeIfPresent([String].self, forKey: .names) ?? []
      written = try c.decodeIfPresent(Int64.self, forKey: .written) ?? 0
      computerName = try c.decodeIfPresent(String.self, forKey: .computerName) ?? ""
      computerState = try c.decodeIfPresent(String.self, forKey: .computerState) ?? "online"
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
  /// opens again because the system may draw the same content more than once); when it does not open, the counts the
  /// state itself carries are drawn. Returns whether the sealed blob was the source.
  func resolved(storePath: String?, boxSecret: Data?) -> (body: ActivityBody, fromSealed: Bool) {
    if let sealed, let storePath, let boxSecret, let packet = Data(urlSafeBase64: sealed),
      let opened = try? openPacket(storePath: storePath, boxSecret: boxSecret, packet: packet), opened.stream == "activity",
      let body = try? JSONDecoder().decode(ActivityBody.self, from: opened.plaintext)
    {
      return (body, true)
    }
    return (ActivityBody(waiting: waiting, names: names, written: written, computerName: computerName, computerState: computerState), false)
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
