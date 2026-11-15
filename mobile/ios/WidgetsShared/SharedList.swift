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
    /// How long the computer still holds the request for phones when `at`, in seconds; at most the fade time counts.
    var holdSecs: Int = 900
    /// When this row was last confirmed, in unix milliseconds (0: when the file was written). A notice refreshes only
    /// its own row, so it can never make another request look fresh.
    var at: Int64 = 0

    init(id: String, name: String, project: String, state: String, pet: String = "default", color: Int = 0, ref: String = "", computer: String = "", holdSecs: Int = 900, at: Int64 = 0) {
      self.id = id; self.name = name; self.project = project; self.state = state; self.pet = pet; self.color = color
      self.ref = ref; self.computer = computer; self.holdSecs = holdSecs; self.at = at
    }

    init(from decoder: Decoder) throws {
      let c = try decoder.container(keyedBy: CodingKeys.self)
      id = try c.decode(String.self, forKey: .id)
      name = try c.decode(String.self, forKey: .name)
      project = try c.decode(String.self, forKey: .project)
      state = try c.decode(String.self, forKey: .state)
      pet = try c.decodeIfPresent(String.self, forKey: .pet) ?? "default"
      color = try c.decodeIfPresent(Int.self, forKey: .color) ?? 0
      ref = try c.decodeIfPresent(String.self, forKey: .ref) ?? ""
      computer = try c.decodeIfPresent(String.self, forKey: .computer) ?? ""
      holdSecs = try c.decodeIfPresent(Int.self, forKey: .holdSecs) ?? 900
      at = try c.decodeIfPresent(Int64.self, forKey: .at) ?? 0
    }
  }

  /// When the app last wrote the whole list, in unix milliseconds.
  var written: Int64
  var computerName: String
  /// `online`, `offline` or `asleep`.
  var computerState: String
  var todo: Int
  var canStart: Bool
  var rows: [Row]

  /// Content older than this is drawn stale whatever state it last showed (the owner's decision: fade on age). With the
  /// app closed nothing tells the phone that the computer went away, so age is the only honest signal.
  static let fade: Int64 = 15 * 60 * 1000

  var waiting: [Row] { rows.filter { $0.state == "waiting" } }
  var working: Int { rows.filter { $0.state == "working" }.count }

  private func start(_ r: Row) -> Int64 { r.at == 0 ? written : r.at }
  private func limit(_ r: Row) -> Int64 { min(Int64(max(r.holdSecs, 1)) * 1000, Self.fade) }

  /// Nothing says "needs you" on old data: a request is stale when it is older than its own wait (at most 15 minutes);
  /// with nothing waiting, the list is stale when it is older than 15 minutes.
  func isStale(at now: Date) -> Bool {
    let n = Int64(now.timeIntervalSince1970 * 1000)
    if waiting.isEmpty { return n - written > Self.fade }
    return waiting.contains { n - start($0) > limit($0) }
  }

  /// The time shown as "Updated hh:mm": when the oldest request waiting (or, with none, the list) was last confirmed.
  var writtenDate: Date {
    let ms = waiting.isEmpty ? written : waiting.map(start).min()!
    return Date(timeIntervalSince1970: Double(ms) / 1000)
  }

  /// The moments after which the verdict of [isStale] changes, so a timeline can dim a surface at the right time
  /// without a reload, and a Live Activity gets its stale date.
  func staleMoments() -> [Date] {
    let ms = waiting.isEmpty ? [written + Self.fade] : waiting.map { start($0) + limit($0) }
    return ms.map { Date(timeIntervalSince1970: Double($0) / 1000 + 0.001) }.sorted()
  }

  /// Hide details on: no session or project name is kept, not only not drawn (the file is readable by the widgets and
  /// survives until the next write).
  mutating func dropNames() {
    for i in rows.indices { rows[i].name = ""; rows[i].project = "" }
  }

  static let empty = SharedList(written: 0, computerName: "", computerState: "offline", todo: 0, canStart: false, rows: [])

  // MARK: a notice

  /// A `waiting` notice puts its request in the list (refreshing that row only), a `closed` one takes it out. `done`
  /// changes nothing. The extension knows only the request's id and the text the computer redacted, so a new row takes
  /// the default pet.
  mutating func apply(kind: String, ref: String, computer: String, title: String, now: Date = Date()) {
    let ms = Int64(now.timeIntervalSince1970 * 1000)
    switch kind {
    case "waiting":
      if let i = rows.firstIndex(where: { $0.ref == ref && $0.computer == computer }) {
        rows[i].state = "waiting"
        rows[i].at = ms
      } else {
        rows.append(Row(id: ref, name: title, project: "", state: "waiting", ref: ref, computer: computer, at: ms))
      }
    case "closed":
      rows.removeAll { $0.ref == ref && $0.computer == computer && $0.state == "waiting" }
      written = ms
    default:
      return
    }
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

  /// The app and the notification extension are two processes: every write is read, change and write under one
  /// exclusive lock, and goes through a temporary file, so a widget reading at that moment sees the old or the new file.
  @discardableResult
  static func update(in dir: URL? = directory(), _ change: (inout SharedList) -> Void) throws -> SharedList {
    guard let dir else { return .empty }
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let fd = open(dir.appendingPathComponent("surfaces.lock").path, O_CREAT | O_RDWR, 0o600)
    guard fd >= 0 else { throw CocoaError(.fileWriteUnknown) }
    defer { close(fd) }
    flock(fd, LOCK_EX)
    defer { flock(fd, LOCK_UN) }
    var list = load(from: dir)
    change(&list)
    if hideDetails(in: dir) { list.dropNames() }
    try JSONEncoder().encode(list).write(to: dir.appendingPathComponent("surfaces.json"), options: .atomic)
    return list
  }

  /// The "Hide details" switch, read from the push store next to the list; a store that cannot be read counts as on.
  static func hideDetails(in dir: URL) -> Bool {
    (try? storeHideDetails(storePath: dir.appendingPathComponent("push.json").path)) ?? true
  }

  /// Replaces the whole list (the app's own `home`).
  static func save(_ list: SharedList, to dir: URL? = directory()) throws {
    try update(in: dir) { $0 = list }
  }

  /// One image per design, colour and state a surface can show: `pets/<design>-<colour>-<state>.png`.
  static func petURL(design: String, color: Int, state: String, in dir: URL? = directory()) -> URL? {
    dir?.appendingPathComponent("pets").appendingPathComponent("\(design)-\(color)-\(state).png")
  }
}
