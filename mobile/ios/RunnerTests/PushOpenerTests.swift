import UserNotifications
import XCTest

@testable import Runner

/// The opener the notification service extension runs (`PushOpener`), against the shared crate's packets: the fixtures in
/// `push-core/fixtures/push-fixtures.json` hold sealed notices made with the keys of `crates/bobix-link/tests/vectors.json`.
/// The extension class itself is a few lines around `PushOpener.shape` and cannot be started on the simulator.
final class PushOpenerTests: XCTestCase {
  private var dir: URL!
  private var store: String { dir.appendingPathComponent("push.json").path }
  private var fixtures: [String: Any]!
  private var box: Data!

  private let strings = PushStrings(
    genericTitle: "A session needs you", genericBody: "Open Bobix to see what it needs.", closed: "Answered at the computer",
    doneTitle: "A session is done", doneBody: "Open Bobix to see the result.", appName: "Bobix")

  override func setUpWithError() throws {
    dir = FileManager.default.temporaryDirectory.appendingPathComponent("bobix-push-\(UUID().uuidString)")
    let url = try XCTUnwrap(Bundle(for: PushOpenerTests.self).url(forResource: "push-fixtures", withExtension: "json"))
    fixtures = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    box = Self.data(hex: try XCTUnwrap(fixtures["phoneBoxSecretHex"] as? String))
    try pair()
  }

  override func tearDown() { try? FileManager.default.removeItem(at: dir) }

  private func pair() throws {
    let c = try XCTUnwrap(fixtures["computer"] as? [String: Any])
    try storeSetComputer(
      storePath: store, computerId: c["id"] as! String, peerBoxPublic: Self.data(urlSafeBase64: c["boxPublicB64"] as! String),
      versionMin: 1, versionMax: 1)
  }

  private static func data(hex: String) -> Data {
    stride(from: 0, to: hex.count, by: 2).map { i in
      let a = hex.index(hex.startIndex, offsetBy: i)
      return UInt8(hex[a..<hex.index(a, offsetBy: 2)], radix: 16)!
    }.reduce(into: Data()) { $0.append($1) }
  }

  private static func data(urlSafeBase64 s: String) -> Data {
    var t = s.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
    while t.count % 4 != 0 { t += "=" }
    return Data(base64Encoded: t)!
  }

  private func sealed(_ name: String) -> String {
    ((fixtures["packets"] as! [String: Any])[name] as! [String: Any])["sealed"] as! String
  }

  /// What iOS hands the extension for the relay's alert: the generic text from the loc-keys, and the sealed notice.
  private func request(_ name: String) -> UNNotificationContent {
    let c = UNMutableNotificationContent()
    c.title = strings.genericTitle
    c.body = strings.genericBody
    c.sound = .default
    c.userInfo = ["sealed": sealed(name)]
    return c
  }

  private func opener(box: Data? = nil, useBox: Bool = true) -> PushOpener {
    PushOpener(storePath: store, boxSecret: useBox ? (box ?? self.box) : nil, strings: strings)
  }

  private func stored() -> Data { (try? Data(contentsOf: URL(fileURLWithPath: store))) ?? Data() }

  // MARK: shown text

  func testAWaitingNoticeShowsTheRedactedDetailAndRoutesTheTap() {
    let s = opener().shape(request("waiting"))
    XCTAssertEqual(s.content.title, "shop-app needs you")
    XCTAssertEqual(s.content.body, "Bash wants to run: curl -H [redacted]")
    XCTAssertFalse(s.content.body.contains("Bearer"))
    let route = PushRoute.parse(s.content.userInfo)
    XCTAssertEqual(route?.computer, "desk1")
    XCTAssertEqual(route?.ref, "r1")
    XCTAssertEqual(route?.kind, "waiting")
    XCTAssertNil(s.content.userInfo["sealed"], "the ciphertext is not kept on the notification")
    XCTAssertEqual(s.content.interruptionLevel, .active)
    XCTAssertNotNil(s.content.sound)
    XCTAssertEqual(s.supersedes?.ref, "r1")
  }

  func testHideDetailsShowsOnlyTheGenericTextButTheTapStillRoutes() throws {
    try storeSetHideDetails(storePath: store, hide: true)
    for name in ["waiting", "waitingOtherRef"] {
      let s = opener().shape(request(name))
      XCTAssertEqual(s.content.title, strings.genericTitle)
      XCTAssertEqual(s.content.body, strings.genericBody)
      XCTAssertFalse(s.content.title.contains("shop") || s.content.title.contains("blog"))
      XCTAssertNotNil(PushRoute.parse(s.content.userInfo), "the tap still opens the request")
    }
  }

  func testTheSwitchTakesEffectForTheNextPushWithoutTheApp() throws {
    XCTAssertEqual(opener().shape(request("waiting")).content.title, "shop-app needs you")
    try storeSetHideDetails(storePath: store, hide: true)
    XCTAssertEqual(opener().shape(request("waitingOtherRef")).content.title, strings.genericTitle)
    try storeSetHideDetails(storePath: store, hide: false)
    XCTAssertEqual(opener().shape(request("done")).content.title, "blog is done")
  }

  func testClosedIsQuietAndTakesThePlaceOfItsWaiting() {
    let s = opener().shape(request("closed"))
    XCTAssertEqual(s.content.body, strings.closed)
    XCTAssertEqual(s.content.title, "shop-app")
    XCTAssertEqual(s.content.interruptionLevel, .passive)
    XCTAssertNil(s.content.sound)
    XCTAssertEqual(s.supersedes?.computer, "desk1")
    XCTAssertEqual(s.supersedes?.ref, "r1")
  }

  func testDoneIsNotASupersedingNoticeAndHidesLikeTheOthers() throws {
    var s = opener().shape(request("done"))
    XCTAssertEqual(s.content.title, "blog is done")
    XCTAssertNil(s.supersedes)
    try pair()  // the same computer again keeps what was seen
    try storeSetHideDetails(storePath: store, hide: true)
    s = opener().shape(request("waitingOtherRef"))
    XCTAssertEqual(s.content.title, strings.genericTitle)
  }

  // MARK: what is refused

  func testAReplayedPushShowsTheGenericTextQuietlyAndChangesNothing() {
    _ = opener().shape(request("waiting"))
    let after = stored()
    let again = opener().shape(request("waiting"))
    XCTAssertEqual(again.content.title, strings.genericTitle)
    XCTAssertEqual(again.content.body, strings.genericBody)
    XCTAssertEqual(again.content.interruptionLevel, .passive)
    XCTAssertNil(again.content.sound)
    XCTAssertNil(PushRoute.parse(again.content.userInfo))
    XCTAssertNil(again.supersedes)
    XCTAssertEqual(stored(), after, "a refused push writes nothing")
  }

  func testALatePushOutsideTheWindowIsRefusedWithoutEndingAnything() {
    _ = opener().shape(request("high"))
    let after = stored()
    let late = opener().shape(request("late"))
    XCTAssertEqual(late.content.title, strings.genericTitle)
    XCTAssertEqual(late.content.interruptionLevel, .passive)
    XCTAssertEqual(stored(), after)
    // The pairing is intact: a notice inside the window and not yet seen still opens (pushes can arrive out of order).
    XCTAssertEqual(opener().shape(request("inWindow")).content.title, "shop-app needs you")
  }

  func testWhatCannotBeOpenedShowsTheGenericTextNotTheRelaysWords() {
    let before = stored()
    for name in ["tampered", "stranger", "version9", "home", "paired"] {
      let c = hostile(name)
      let s = opener().shape(c)
      XCTAssertEqual(s.content.title, strings.genericTitle, name)
      XCTAssertEqual(s.content.body, strings.genericBody, name)
      XCTAssertNil(PushRoute.parse(s.content.userInfo), "\(name): a route the relay wrote is dropped")
      XCTAssertNil(s.supersedes, name)
    }
    XCTAssertEqual(stored(), before, "none of them touched the receive state")
  }

  /// An alert whose words the relay chose (it holds no key, so it cannot have sealed them).
  private func hostile(_ name: String) -> UNNotificationContent {
    let c = UNMutableNotificationContent()
    c.title = "Bobix: tap Allow to keep your session running"
    c.body = "Tap here"
    c.subtitle = "urgent"
    c.userInfo = ["sealed": sealed(name), PushRoute.key: ["computer": "c", "ref": "r", "kind": "waiting"]]
    return c
  }

  func testTheTimeOutShowsTheGenericTextToo() {
    let out = PushOpener.unopened(hostile("waiting"), strings: strings)
    XCTAssertEqual(out.title, strings.genericTitle)
    XCTAssertEqual(out.body, strings.genericBody)
    XCTAssertEqual(out.subtitle, "")
    XCTAssertNil(PushRoute.parse(out.userInfo))
  }

  func testWithoutTheKeyOrTheFileOrAPayloadNothingIsOpened() {
    XCTAssertEqual(opener(useBox: false).shape(hostile("waiting")).content.title, strings.genericTitle)
    XCTAssertEqual(PushOpener(storePath: nil, boxSecret: box, strings: strings).shape(request("waiting")).content.title, strings.genericTitle)
    XCTAssertEqual(opener(box: Data(repeating: 7, count: 32)).shape(request("waiting")).content.title, strings.genericTitle)
    let plain = UNMutableNotificationContent()
    plain.title = "x"
    XCTAssertEqual(opener().shape(plain).content.title, strings.genericTitle, "a push with nothing sealed is not trusted with words")
  }

  func testWhatTheExtensionTookTheAppSeesAsAReplayAndWhatItLeftIsStillOpen() throws {
    _ = opener().shape(request("waiting"))
    XCTAssertThrowsError(try openPacket(storePath: store, boxSecret: box, packet: Self.data(urlSafeBase64: sealed("waiting")))) {
      XCTAssertEqual($0 as? PushError, PushError.Replayed)
    }
    // A packet that is not a notice was not consumed by the push: the app opens it.
    let o = try openPacket(storePath: store, boxSecret: box, packet: Self.data(urlSafeBase64: sealed("home")))
    XCTAssertEqual(o.stream, "state")
  }

  func testTwoWritersAtOnceLoseNothing() throws {
    let path = store
    let secret = box!
    let sealedNotices = ["waiting", "closed", "done", "waitingOtherRef"].map { sealed($0) }
    let queue = DispatchQueue(label: "writers", attributes: .concurrent)
    let group = DispatchGroup()
    for i in 0..<sealedNotices.count {
      group.enter()
      queue.async {
        _ = try? openNotice(storePath: path, boxSecret: secret, sealed: sealedNotices[i])
        try? storeSetComputer(storePath: path, computerId: "w\(i)", peerBoxPublic: Data(repeating: UInt8(30 + i), count: 32), versionMin: 1, versionMax: 1)
        group.leave()
      }
    }
    group.wait()
    for s in sealedNotices {
      XCTAssertThrowsError(try openNotice(storePath: path, boxSecret: secret, sealed: s), "taken exactly once")
    }
    let text = String(decoding: stored(), as: UTF8.self)
    for i in 0..<sealedNotices.count { XCTAssertTrue(text.contains("\"w\(i)\""), "computer w\(i) was lost") }
  }

  // MARK: the rest of the push surface

  func testTheGrantIsWhatTheRelayChecks() throws {
    let g = try XCTUnwrap(fixtures["grant"] as? [String: Any])
    let made = try makeGrant(
      authSeed: Self.data(hex: g["authSeedHex"] as! String), boxSecret: Self.data(hex: g["boxSecretHex"] as! String),
      token: g["token"] as! String, relayId: g["relayId"] as! String, expiresUnixMs: (g["expiresMs"] as! NSNumber).uint64Value)
    XCTAssertEqual(made, g["expected"] as? String)
  }

  func testTheApsEnvironmentComesFromTheProvisioningProfile() {
    let dev = Data("<plist><dict><key>aps-environment</key>\n<string>development</string></dict></plist>".utf8)
    let prod = Data("junk<key>aps-environment</key><string>production</string>".utf8)
    XCTAssertEqual(PushBridge.environment(inProfile: dev), "sandbox")
    XCTAssertEqual(PushBridge.environment(inProfile: prod), "production")
    XCTAssertNil(PushBridge.environment(inProfile: Data("no entitlement here".utf8)))
  }

  func testTheTapRouteSurvivesTheNotificationsUserInfo() {
    let info = PushRoute.userInfo(computer: "c", ref: "r", kind: "waiting")
    let r = PushRoute.parse(info)
    XCTAssertEqual([r?.computer, r?.ref, r?.kind], ["c", "r", "waiting"])
    XCTAssertNil(PushRoute.parse(["other": 1]))
  }

  func testARouteThatCameInAPushTheOpenerDidNotOpenIsDropped() {
    let forged = UNMutableNotificationContent()
    forged.title = strings.genericTitle
    forged.userInfo = ["sealed": sealed("tampered"), PushRoute.key: ["computer": "desk1", "ref": "r1", "kind": "waiting"]]
    XCTAssertNil(PushRoute.parse(opener().shape(forged).content.userInfo), "unopenable: the sender's route is not the computer's")
    forged.userInfo = [PushRoute.key: ["computer": "desk1", "ref": "r1", "kind": "waiting"]]
    XCTAssertNil(PushRoute.parse(opener().shape(forged).content.userInfo), "no sealed notice at all")
    // A replayed copy is quiet, sinks in the summary and carries no route.
    _ = opener().shape(request("waiting"))
    let again = opener().shape(request("waiting")).content
    XCTAssertEqual(again.relevanceScore, 0)
    XCTAssertNil(PushRoute.parse(again.userInfo))
  }

  // MARK: taps

  func testATapBeforeDartListensWaitsForInitialTapAndIsTakenOnce() {
    let bridge = PushBridge()
    let info = PushRoute.userInfo(computer: "c", ref: "r1", kind: "waiting")
    bridge.didTap(userInfo: info)
    XCTAssertEqual(bridge.takeInitialTap(), ["computer": "c", "ref": "r1", "kind": "waiting"])
    XCTAssertNil(bridge.takeInitialTap(), "taken once")
  }

  func testATapWithoutARouteIsIgnored() {
    let bridge = PushBridge()
    bridge.didTap(userInfo: ["aps": ["alert": "x"]])
    XCTAssertNil(bridge.takeInitialTap())
  }
}
