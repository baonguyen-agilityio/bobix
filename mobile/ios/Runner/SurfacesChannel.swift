import ActivityKit
import Flutter
import Foundation
import UserNotifications
import WidgetKit

/// The platform channel `dev.bobix.phone/surfaces`: the app writes what the widgets, the Live Activity and the controls
/// show (the shared list and the pets' images in the App Group), asks for their timelines to reload, and starts, updates
/// and ends the Live Activity. Tokens of the activity go back to Dart, which hands them to the relay.
final class SurfacesBridge {
  private var channel: FlutterMethodChannel?
  /// The activities whose token and state this process already watches.
  private var observed = Set<String>()
  private var startTokenTask: Task<Void, Never>?
  /// The last tokens sent to Dart. The system hands them over as soon as the app starts, often before Dart listens, so Dart
  /// asks for them again (`replayTokens`) once it does.
  private var lastStartToken: String?
  private var activityTokens: [String: String] = [:]

  func attach(messenger: FlutterBinaryMessenger) {
    let c = FlutterMethodChannel(name: "dev.bobix.phone/surfaces", binaryMessenger: messenger)
    c.setMethodCallHandler { [weak self] call, result in self?.handle(call, result) }
    channel = c
    watchPushToStartToken()
    // An activity from an earlier run of the app is still alive: watch it again instead of starting another.
    for a in Activity<BobixActivityAttributes>.activities { observe(a) }
    watchStartedActivities()
  }

  /// An activity that a push started (the app is woken in the background to learn of it): its own push token must reach the
  /// relay too, or later updates for it have nowhere to go.
  private func watchStartedActivities() {
    Task { [weak self] in
      for await a in Activity<BobixActivityAttributes>.activityUpdates { self?.observe(a) }
    }
  }

  private func replayTokens() {
    if let t = lastStartToken { channel?.invokeMethod("pushToStartToken", arguments: t) }
    for (id, token) in activityTokens { channel?.invokeMethod("activityToken", arguments: ["id": id, "token": token]) }
  }

  /// A new push-to-start token (iOS 17.2 and later) goes to Dart, which registers it with the relay as `pushToStart`.
  private func watchPushToStartToken() {
    guard #available(iOS 17.2, *), startTokenTask == nil else { return }
    startTokenTask = Task { [weak self] in
      for await t in Activity<BobixActivityAttributes>.pushToStartTokenUpdates {
        let hex = t.map { String(format: "%02x", $0) }.joined()
        await MainActor.run {
          self?.lastStartToken = hex
          self?.channel?.invokeMethod("pushToStartToken", arguments: hex)
        }
      }
    }
  }

  /// Watches an activity's own push token (Dart registers it with the relay) and tells Dart when it ended, so its token
  /// is not registered again.
  private func observe(_ a: Activity<BobixActivityAttributes>) {
    guard observed.insert(a.id).inserted else { return }
    Task { [weak self] in
      for await t in a.pushTokenUpdates {
        let hex = t.map { String(format: "%02x", $0) }.joined()
        await MainActor.run {
          self?.activityTokens[a.id] = hex
          self?.channel?.invokeMethod("activityToken", arguments: ["id": a.id, "token": hex])
        }
      }
    }
    Task { [weak self] in
      for await st in a.activityStateUpdates where st == .ended || st == .dismissed {
        await MainActor.run {
          self?.activityTokens[a.id] = nil
          self?.channel?.invokeMethod("activityEnded", arguments: a.id)
        }
        self?.observed.remove(a.id)
        break
      }
    }
  }

  static func reloadSurfaces() {
    WidgetCenter.shared.reloadAllTimelines()
    ControlCenter.shared.reloadAllControls()
  }

  /// The pairing ended (Disconnect, Erase, a `revoked` from the computer): nothing of the computer stays on the Home
  /// Screen, the Lock Screen or in the Dynamic Island. The list is emptied, the pets' images are deleted, every activity
  /// ends and the surfaces reload.
  static func clear(in dir: URL? = SharedListStore.directory(), endActivities: Bool = true) {
    if let dir {
      try? SharedListStore.update(in: dir) { $0 = .empty }
      try? FileManager.default.removeItem(at: dir.appendingPathComponent("pets"))
    }
    if endActivities {
      Task { for a in Activity<BobixActivityAttributes>.activities { await a.end(nil, dismissalPolicy: .immediate) } }
    }
    reloadSurfaces()
  }

  /// Draws every running activity again (a setting such as Hide details changed what it shows).
  static func refreshActivities() {
    Task { for a in Activity<BobixActivityAttributes>.activities { await a.update(a.content) } }
  }

  /// A notice that came without an alert (a `closed` push is sent that way): open it like the extension does and
  /// update the surfaces. Returns whether a notice opened.
  @discardableResult
  static func applyPush(_ userInfo: [AnyHashable: Any]) -> Bool {
    guard let sealed = userInfo["sealed"] as? String else { return false }
    let content = UNMutableNotificationContent()
    content.userInfo = ["sealed": sealed]
    guard let n = PushOpener.live().shape(content).notice else { return false }
    try? SharedListStore.update { $0.apply(kind: n.kind, ref: n.ref, computer: n.computer, title: n.title) }
    reloadSurfaces()
    refreshActivities()
    return true
  }

  private func state(_ args: [String: Any]) -> BobixActivityAttributes.ContentState {
    BobixActivityAttributes.ContentState(waiting: args["waiting"] as? Int ?? 0, written: (args["written"] as? NSNumber)?.int64Value ?? 0)
  }

  /// One activity, never two: nothing waiting ends every activity; something waiting updates the one that exists (and
  /// ends any extra) or starts one. The stale date is the last update plus 15 minutes, so the system draws it stale.
  private func syncActivity(_ args: [String: Any], _ result: @escaping FlutterResult) {
    let all = Activity<BobixActivityAttributes>.activities
    let s = state(args)
    let stale = Date(timeIntervalSince1970: Double(s.written + SharedList.fade) / 1000)
    Task {
      if s.waiting == 0 {
        for a in all { await a.end(nil, dismissalPolicy: .immediate) }
        return result(["active": false])
      }
      for extra in all.dropFirst() { await extra.end(nil, dismissalPolicy: .immediate) }
      if let a = all.first {
        await a.update(.init(state: s, staleDate: stale))
        observe(a)
        return result(["active": true, "id": a.id])
      }
      guard ActivityAuthorizationInfo().areActivitiesEnabled else { return result(["active": false, "reason": "disabled"]) }
      do {
        let a = try Activity.request(
          attributes: BobixActivityAttributes(computerId: args["computerId"] as? String ?? ""), content: .init(state: s, staleDate: stale), pushType: .token)
        observe(a)
        result(["active": true, "id": a.id])
      } catch { result(["active": false, "reason": "\(error)"]) }
    }
  }

  private func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    let args = call.arguments as? [String: Any] ?? [:]
    func fail(_ m: String) { result(FlutterError(code: "surfaces_error", message: m, details: nil)) }
    switch call.method {
    case "capabilities":
      result(["liveActivities": ActivityAuthorizationInfo().areActivitiesEnabled])
    case "writeList":
      // Decoded before it is kept: a file the widgets cannot read would blank every surface.
      guard let json = (args["json"] as? String)?.data(using: .utf8), let list = try? JSONDecoder().decode(SharedList.self, from: json) else {
        return fail("the list is not a list")
      }
      do { try SharedListStore.save(list); SurfacesBridge.reloadSurfaces(); result(nil) } catch { fail("\(error)") }
    case "writePet":
      guard let design = args["design"] as? String, let color = args["color"] as? Int, (0..<64).contains(color), let state = args["state"] as? String, let png = (args["png"] as? FlutterStandardTypedData)?.data,
        design.range(of: "^[a-z0-9-]{1,32}$", options: .regularExpression) != nil, state.range(of: "^[a-z]{1,16}$", options: .regularExpression) != nil,
        let url = SharedListStore.petURL(design: design, color: color, state: state)
      else { return fail("bad pet image") }
      do {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try png.write(to: url, options: .atomic)
        result(nil)
      } catch { fail("\(error)") }
    case "syncActivity": syncActivity(args, result)
    case "replayTokens": replayTokens(); result(nil)
    case "clearSurfaces": SurfacesBridge.clear(); result(nil)
    default: result(FlutterMethodNotImplemented)
    }
  }
}
