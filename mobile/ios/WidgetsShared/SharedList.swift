import Foundation

/// What the widgets, the Live Activity and the controls show, in a file in the App Group that the app and the
/// notification extension write and the widget extension reads. It holds names and counts only: never a tool input, a
/// command or a key. Who writes it asks for a timeline reload afterwards; iOS budgets reloads, so a surface may show
/// the file a little later, which is what the stale state is for.
struct SharedList: Codable, Equatable {
  struct Row: Codable, Equatable {
    var id: String
    /// The session's name and its project (the redacted names the computer already sends in `home`).
    var name: String
    var project: String
    /// `waiting`, `working`, `idle`, `failed` or `done`: the pet's state.
    var state: String
    var pet: String = "default"
    var color: Int = 0
    /// What a tap opens: the held item's id, empty when nothing is held for this phone.
    var ref: String = ""
    var computer: String = ""
    /// How long the computer holds the request for phones, in seconds: after that the row may be gone.
    var holdSecs: Int = 600
  }

  /// When the file was written, in unix milliseconds.
  var written: Int64
  var computerName: String
  /// `online`, `offline` or `asleep`.
  var computerState: String
  var todo: Int
  var canStart: Bool
  var rows: [Row]

  static let staleAfterOffline: Int64 = 15 * 60 * 1000

  var waiting: [Row] { rows.filter { $0.state == "waiting" } }
  var working: Int { rows.filter { $0.state == "working" }.count }

  /// Nothing says "needs you" on old data: a surface is stale when its data is older than the request's wait, or older
  /// than 15 minutes while the computer is not online.
  func isStale(at now: Date) -> Bool {
    let age = Int64(now.timeIntervalSince1970 * 1000) - written
    if computerState != "online" && age > Self.staleAfterOffline { return true }
    return waiting.contains { age > Int64($0.holdSecs) * 1000 }
  }

  var writtenDate: Date { Date(timeIntervalSince1970: Double(written) / 1000) }

  /// The moments after which the verdict of [isStale] changes, so a timeline can dim a surface at the right time
  /// without a reload.
  func staleMoments() -> [Date] {
    var ms = waiting.map { written + Int64($0.holdSecs) * 1000 }
    if computerState != "online" { ms.append(written + Self.staleAfterOffline) }
    return ms.map { Date(timeIntervalSince1970: Double($0) / 1000 + 0.001) }.sorted()
  }

  static let empty = SharedList(written: 0, computerName: "", computerState: "offline", todo: 0, canStart: false, rows: [])

  // MARK: the notification extension's part

  /// A `waiting` notice puts its request in the list, a `closed` one takes it out. `done` changes nothing. The
  /// extension knows only the request's id and the text the computer redacted, so a new row takes the default pet.
  mutating func apply(kind: String, ref: String, computer: String, title: String, now: Date = Date()) {
    switch kind {
    case "waiting":
      let row = Row(id: ref, name: title, project: "", state: "waiting", ref: ref, computer: computer)
      if let i = rows.firstIndex(where: { $0.ref == ref && $0.computer == computer }) { rows[i].state = "waiting" } else { rows.append(row) }
    case "closed":
      rows.removeAll { $0.ref == ref && $0.computer == computer && $0.state == "waiting" }
    default:
      return
    }
    written = Int64(now.timeIntervalSince1970 * 1000)
  }
}

enum SharedListStore {
  /// `surfaces.json` next to the push store, and the pets' images in `pets/`.
  static func directory() -> URL? {
    PushPaths.storePath().map { URL(fileURLWithPath: $0).deletingLastPathComponent() }
  }

  static func load(from dir: URL? = directory()) -> SharedList {
    guard let dir, let data = try? Data(contentsOf: dir.appendingPathComponent("surfaces.json")),
      let list = try? JSONDecoder().decode(SharedList.self, from: data)
    else { return .empty }
    return list
  }

  /// Written through a temporary file, so a widget reading at that moment sees the old or the new file, never half.
  static func save(_ list: SharedList, to dir: URL? = directory()) throws {
    guard let dir else { return }
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    try JSONEncoder().encode(list).write(to: dir.appendingPathComponent("surfaces.json"), options: .atomic)
  }

  /// One image per design, colour and state a surface can show: `pets/<design>-<colour>-<state>.png`.
  static func petURL(design: String, color: Int, state: String, in dir: URL? = directory()) -> URL? {
    dir?.appendingPathComponent("pets").appendingPathComponent("\(design)-\(color)-\(state).png")
  }
}
