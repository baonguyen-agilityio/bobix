import SwiftUI
import WidgetKit
import XCTest

@testable import Runner

/// The shared list, the links and every surface the widget extension draws, with made-up data. The views are the ones
/// the extension uses (`WidgetsShared`); the tests draw each of them in each state, in light and dark, in English and
/// Vietnamese, and write the pictures to `BOBIX_SURFACE_SHOTS` (default: a temporary folder) for a person to look at.
final class SurfacesTests: XCTestCase {
  private let now = Date(timeIntervalSince1970: 1_800_000_000)
  private var ms: Int64 { Int64(now.timeIntervalSince1970 * 1000) }

  private func list(waiting: Int = 2, state: String = "online", ageMinutes: Double = 1, holdSecs: Int = 600) -> SharedList {
    let names = [("Add a dark mode switch", "shop"), ("Speed up the search page", "blog"), ("Move the docs", "blog")]
    var rows = (0..<waiting).map { i in
      SharedList.Row(id: "w\(i)", name: names[i % 3].0, project: names[i % 3].1, state: "waiting", pet: ["default", "nota", "tty"][i % 3], color: i, ref: "r\(i)", computer: "c1", holdSecs: holdSecs)
    }
    rows.append(.init(id: "x", name: "Fix the cart total", project: "shop", state: "working"))
    return SharedList(
      written: ms - Int64(ageMinutes * 60_000), computerName: "Studio Mac", computerState: state, todo: 2, canStart: true, rows: rows)
  }

  // MARK: the list

  func testSurfacesFadeOnAgeWhateverTheLastStateWas() {
    XCTAssertFalse(list().isStale(at: now), "fresh data is not stale")
    XCTAssertTrue(list(ageMinutes: 11, holdSecs: 600).isStale(at: now), "older than the request's wait")
    XCTAssertFalse(list(ageMinutes: 14, holdSecs: 3600).isStale(at: now), "a long wait still fades at 15 minutes, not before")
    XCTAssertTrue(list(ageMinutes: 16, holdSecs: 3600).isStale(at: now), "older than 15 minutes whatever the wait")
    XCTAssertTrue(list(waiting: 0, ageMinutes: 16).isStale(at: now), "nothing waiting, but old: the computer may be gone")
    XCTAssertFalse(list(waiting: 0, ageMinutes: 14).isStale(at: now))
    XCTAssertTrue(list(state: "online", ageMinutes: 16).isStale(at: now), "an online computer seen 16 minutes ago")
  }

  func testTheTimelineDimsOnTimeWithoutAReload() {
    let moments = list().staleMoments()
    XCTAssertEqual(moments.count, 2, "each waiting request's own moment")
    XCTAssertTrue(zip(moments, moments.dropFirst()).allSatisfy { $0 <= $1 })
    XCTAssertTrue(list().isStale(at: moments[0].addingTimeInterval(1)))
    XCTAssertEqual(list(waiting: 0).staleMoments().count, 1)
  }

  func testANoticeRefreshesOnlyItsOwnRow() {
    var l = list(waiting: 1, ageMinutes: 14)  // r0 was confirmed 14 minutes ago
    l.apply(kind: "waiting", ref: "newer", computer: "c1", title: "blog needs you", now: now)
    XCTAssertEqual(l.waiting.count, 2)
    XCTAssertEqual(l.waiting[1].at, ms)
    XCTAssertEqual(l.written, ms - 14 * 60_000, "the file-wide time is the app's, not the notice's")
    XCTAssertTrue(l.isStale(at: now.addingTimeInterval(2 * 60)), "the old request still ages: the new one did not freshen it")
    l.apply(kind: "waiting", ref: "newer", computer: "c1", title: "again", now: now.addingTimeInterval(5))
    XCTAssertEqual(l.waiting.count, 2, "the same request twice is one row")
    l.apply(kind: "done", ref: "x", computer: "c1", title: "x", now: now.addingTimeInterval(6))
    XCTAssertEqual(l.waiting.count, 2, "a done notice changes nothing")
    l.apply(kind: "closed", ref: "newer", computer: "c1", title: "", now: now.addingTimeInterval(9))
    XCTAssertEqual(l.waiting.count, 1)
    XCTAssertEqual(l.written, ms + 9000)
  }

  func testTheFileHoldsNamesAndCountsOnly() throws {
    let json = try JSONSerialization.jsonObject(with: JSONEncoder().encode(list())) as! [String: Any]
    XCTAssertEqual(Set(json.keys), ["written", "computerName", "computerState", "todo", "canStart", "rows"])
    let row = (json["rows"] as! [[String: Any]])[0]
    XCTAssertEqual(Set(row.keys), ["id", "name", "project", "state", "pet", "color", "ref", "computer", "holdSecs", "at"], "no tool input, no command")
  }

  func testTheListSurvivesAWriteAndARead() throws {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("surfaces-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: dir) }
    XCTAssertEqual(SharedListStore.load(from: dir), .empty, "no file: nothing waits")
    try SharedListStore.save(list(), to: dir)
    XCTAssertEqual(SharedListStore.load(from: dir), list())
    try Data("{broken".utf8).write(to: dir.appendingPathComponent("surfaces.json"))
    XCTAssertEqual(SharedListStore.load(from: dir), .empty, "a file that cannot be read blanks the surface, never crashes it")
  }

  func testTwoWritersAtOnceLoseNothingInTheList() throws {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("surfaces-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: dir) }
    let group = DispatchGroup()
    let queue = DispatchQueue(label: "w", attributes: .concurrent)
    for i in 0..<8 {
      group.enter()
      queue.async {
        try? SharedListStore.update(in: dir) { $0.apply(kind: "waiting", ref: "r\(i)", computer: "c", title: "t", now: Date()) }
        group.leave()
      }
    }
    group.wait()
    XCTAssertEqual(SharedListStore.load(from: dir).waiting.count, 8, "every update survived: read, change, write under one lock")
  }

  // MARK: links and the activity

  func testALinkOnlyPicksAScreen() {
    XCTAssertEqual(SurfaceLink.tap(URL(string: "bobix://request?computer=c1&ref=r1")!), ["computer": "c1", "ref": "r1", "kind": "waiting"])
    XCTAssertEqual(SurfaceLink.tap(URL(string: "bobix://start")!)?["kind"], "start")
    XCTAssertEqual(SurfaceLink.tap(URL(string: "bobix://home")!)?["kind"], "home")
    for bad in ["bobix://request?computer=c1", "bobix://pair?x=1", "bobix://allow?ref=r1", "https://example.test/home", "bobix://"] {
      XCTAssertNil(SurfaceLink.tap(URL(string: bad)!), bad)
    }
    XCTAssertLessThanOrEqual(SurfaceLink.tap(URL(string: "bobix://request?computer=c&ref=" + String(repeating: "a", count: 999))!)?["ref"]?.count ?? 0, 256)
  }

  func testThePushContentStateNeedsOnlyTheSealedKey() throws {
    let s = try JSONDecoder().decode(BobixActivityAttributes.ContentState.self, from: Data(#"{"sealed":"AAAA"}"#.utf8))
    XCTAssertEqual(s.sealed, "AAAA")
    XCTAssertEqual(s.waiting, 0)
  }

  func testWithoutASealedBlobTheActivityDrawsTheSharedListNotTheState() {
    let s = BobixActivityAttributes.ContentState(waiting: 99, written: ms)
    let (l, fromSealed) = s.resolved(storePath: nil, boxSecret: nil, file: list(waiting: 2), now: now)
    XCTAssertFalse(fromSealed)
    XCTAssertEqual(l.waiting.count, 2, "the count in the state is a hint for the system, the list is what is drawn")
    XCTAssertEqual(l.waiting.first?.name, "Add a dark mode switch")
  }

  func testASealedBlobThatDoesNotOpenDrawsNothingNeedsYouAndNoRelaySuppliedText() throws {
    // A relay puts what it likes in a content state: names and a computer it made up are never drawn.
    let json = #"{"sealed":"AAAA","waiting":5,"names":["Approve: deploy to production"],"computerName":"Your bank"}"#
    let s = try JSONDecoder().decode(BobixActivityAttributes.ContentState.self, from: Data(json.utf8))
    let (l, fromSealed) = s.resolved(storePath: nil, boxSecret: nil, file: list(waiting: 2), now: now)
    XCTAssertFalse(fromSealed)
    XCTAssertTrue(l.waiting.isEmpty)
    XCTAssertEqual(l.computerName, "")
    let m = SurfaceModel(list: l, hide: false, now: now, text: .table(["phone.wg.none": "Nothing needs you"]), petDir: nil)
    XCTAssertEqual(m.mode, .none)
    XCTAssertEqual(m.headline, "Nothing needs you")
  }

  func testTheSystemsStaleFlagDrawsTheActivityStaleWhateverTheListSays() throws {
    var m = try model(list(waiting: 2))
    XCTAssertEqual(m.mode, .needs)
    m.forceStale = true
    XCTAssertEqual(m.mode, .stale)
    XCTAssertTrue(m.rows(3).isEmpty)
    XCTAssertEqual(m.url.absoluteString, "bobix://home")
  }

  // MARK: what each surface says

  private func text(_ lang: String) throws -> SurfaceText {
    let bundle = Bundle(for: SurfacesTests.self)
    let path = try XCTUnwrap(bundle.path(forResource: "Localizable", ofType: "strings", inDirectory: nil, forLocalization: lang))
    return .table(try XCTUnwrap(NSDictionary(contentsOfFile: path) as? [String: String]))
  }

  private func model(_ l: SharedList, hide: Bool = false, lang: String = "en") throws -> SurfaceModel {
    SurfaceModel(list: l, hide: hide, now: now, text: try text(lang), petDir: petDir)
  }

  func testOldDataNeverSaysNeedsYouAndSaysWhenItWasUpdated() throws {
    for lang in ["en", "vi"] {
      let stale = try model(list(ageMinutes: 30), lang: lang)
      XCTAssertEqual(stale.mode, .stale)
      XCTAssertTrue(stale.headline.contains(SurfaceText.clock(stale.list.writtenDate)), "\(lang): the time it was written")
      XCTAssertTrue(stale.rows(5).isEmpty, "\(lang): no names on old data")
      XCTAssertEqual(stale.url.absoluteString, "bobix://home", "old data opens Home, not a request that may be gone")
    }
    XCTAssertEqual(try model(list(ageMinutes: 30)).headline.hasPrefix("Updated"), true)
    XCTAssertEqual(try model(list(ageMinutes: 30), lang: "vi").headline.hasPrefix("Cập nhật"), true)
  }

  func testHideDetailsShowsCountsOnly() throws {
    for lang in ["en", "vi"] {
      let m = try model(list(waiting: 3), hide: true, lang: lang)
      XCTAssertTrue(m.rows(5).isEmpty)
      XCTAssertFalse(m.headline.contains("shop") || m.headline.contains("blog"))
      XCTAssertEqual(m.count, 3)
    }
  }

  func testTheCountAndTheLinks() throws {
    XCTAssertEqual(try model(list(waiting: 1)).headline, "1 needs you")
    XCTAssertEqual(try model(list(waiting: 3)).headline, "3 need you")
    XCTAssertEqual(try model(list(waiting: 3), lang: "vi").headline, "3 phiên cần bạn")
    XCTAssertEqual(try model(list(waiting: 0)).headline, "Nothing needs you")
    XCTAssertEqual(try model(list(waiting: 0)).url.absoluteString, "bobix://home", "nothing waits: Home, not a missing request")
    XCTAssertEqual(try model(list(waiting: 2)).url.absoluteString, "bobix://request?computer=c1&ref=r0")
  }

  func testEveryKeyTheSurfacesUseIsTranslated() throws {
    let en = try XCTUnwrap(NSDictionary(contentsOfFile: try XCTUnwrap(Bundle(for: SurfacesTests.self).path(forResource: "Localizable", ofType: "strings", inDirectory: nil, forLocalization: "en"))) as? [String: String])
    let vi = try XCTUnwrap(NSDictionary(contentsOfFile: try XCTUnwrap(Bundle(for: SurfacesTests.self).path(forResource: "Localizable", ofType: "strings", inDirectory: nil, forLocalization: "vi"))) as? [String: String])
    XCTAssertEqual(Set(en.keys), Set(vi.keys))
    for k in ["phone.wg.need.1", "phone.wg.need.n", "phone.wg.none", "phone.wg.updated", "phone.wg.control.start", "phone.wg.nostart"] { XCTAssertNotNil(en[k], k) }
  }

  // MARK: the pictures

  private var petDir: URL? {
    ProcessInfo.processInfo.environment["BOBIX_PET_DIR"].map { URL(fileURLWithPath: $0) }
  }

  private var shots: URL {
    let d = ProcessInfo.processInfo.environment["BOBIX_SURFACE_SHOTS"].map { URL(fileURLWithPath: $0) }
      ?? FileManager.default.temporaryDirectory.appendingPathComponent("bobix-surfaces")
    try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
    return d
  }

  @MainActor
  private func draw<V: View>(_ name: String, _ size: CGSize, dark: Bool, lock: Bool = false, @ViewBuilder _ view: () -> V) throws {
    let background = lock ? (dark ? Color(white: 0.25) : Color(white: 0.8)) : (dark ? Color(white: 0.11) : Color(white: 0.95))
    let content = view()
      .padding(lock ? 6 : 14)
      .frame(width: size.width, height: size.height, alignment: .topLeading)
      .background(background)
      .environment(\.colorScheme, dark ? .dark : .light)
      .environment(\.locale, Locale(identifier: name.contains("-vi-") ? "vi" : "en"))
    let renderer = ImageRenderer(content: content)
    renderer.scale = 3
    let image = try XCTUnwrap(renderer.uiImage, name)
    let png = try XCTUnwrap(image.pngData())
    XCTAssertGreaterThan(png.count, 800, "\(name) drew something")
    try png.write(to: shots.appendingPathComponent("\(name).png"))
  }

  @MainActor
  func testEveryFamilyInEveryStateLightAndDarkInBothLanguages() throws {
    let states: [(String, SharedList, Bool)] = [
      ("needs", list(waiting: 3), false), ("none", list(waiting: 0), false), ("stale", list(ageMinutes: 25), false),
      ("hidden", list(waiting: 3), true),
    ]
    for lang in ["en", "vi"] {
      for dark in [false, true] {
        for (state, l, hide) in states {
          let m = try model(l, hide: hide, lang: lang)
          let tag = "\(lang)-\(dark ? "dark" : "light")-\(state)"
          try draw("home-small-\(tag)", CGSize(width: 170, height: 170), dark: dark) { HomeSmallView(model: m) }
          try draw("home-medium-\(tag)", CGSize(width: 364, height: 170), dark: dark) { HomeMediumView(model: m) }
          try draw("home-large-\(tag)", CGSize(width: 364, height: 382), dark: dark) { HomeLargeView(model: m) }
          try draw("lock-inline-\(tag)", CGSize(width: 260, height: 30), dark: dark, lock: true) { LockInlineView(model: m) }
          try draw("lock-circular-\(tag)", CGSize(width: 84, height: 84), dark: dark, lock: true) { LockCircularView(model: m) }
          try draw("lock-rect-\(tag)", CGSize(width: 190, height: 84), dark: dark, lock: true) { LockRectangularView(model: m) }
          try draw("activity-banner-\(tag)", CGSize(width: 364, height: 96), dark: dark, lock: true) { ActivityBannerView(model: m) }
          try draw("island-compact-\(tag)", CGSize(width: 150, height: 38), dark: true) {
            HStack { IslandCompactLeading(model: m); Spacer(); IslandCount(model: m) }
          }
        }
      }
    }
  }
}
