import ActivityKit
import Flutter
import Foundation
import WidgetKit

/// The platform channel `dev.bobix.phone/surfaces`: the app writes what the widgets, the Live Activity and the controls
/// show (the shared list and the pets' images in the App Group), asks for their timelines to reload, and starts, updates
/// and ends the Live Activity. Tokens of the activity go back to Dart, which hands them to the relay.
final class SurfacesBridge {
  private var channel: FlutterMethodChannel?
  private var activity: Activity<BobixActivityAttributes>?
  private var tokenTask: Task<Void, Never>?
  private var startTokenTask: Task<Void, Never>?

  func attach(messenger: FlutterBinaryMessenger) {
    let c = FlutterMethodChannel(name: "dev.bobix.phone/surfaces", binaryMessenger: messenger)
    c.setMethodCallHandler { [weak self] call, result in self?.handle(call, result) }
    channel = c
    watchPushToStartToken()
  }

  /// A new push-to-start token (iOS 17.2 and later) goes to Dart, which registers it with the relay as `pushToStart`.
  private func watchPushToStartToken() {
    guard #available(iOS 17.2, *), startTokenTask == nil else { return }
    startTokenTask = Task { [weak self] in
      for await t in Activity<BobixActivityAttributes>.pushToStartTokenUpdates {
        let hex = t.map { String(format: "%02x", $0) }.joined()
        await MainActor.run { self?.channel?.invokeMethod("pushToStartToken", arguments: hex) }
      }
    }
  }

  static func reloadSurfaces() {
    WidgetCenter.shared.reloadAllTimelines()
    ControlCenter.shared.reloadAllControls()
  }

  private func state(_ args: [String: Any]) -> BobixActivityAttributes.ContentState {
    BobixActivityAttributes.ContentState(
      waiting: args["waiting"] as? Int ?? 0, names: args["names"] as? [String] ?? [], written: (args["written"] as? NSNumber)?.int64Value ?? 0,
      computerName: args["computerName"] as? String ?? "", computerState: args["computerState"] as? String ?? "online")
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
    case "startActivity":
      guard ActivityAuthorizationInfo().areActivitiesEnabled else { return result(["started": false, "reason": "disabled"]) }
      let attrs = BobixActivityAttributes(computerId: args["computerId"] as? String ?? "")
      do {
        let a = try Activity.request(
          attributes: attrs, content: .init(state: state(args), staleDate: nil), pushType: .token)
        activity = a
        tokenTask?.cancel()
        tokenTask = Task { [weak self] in
          for await t in a.pushTokenUpdates {
            let hex = t.map { String(format: "%02x", $0) }.joined()
            await MainActor.run { self?.channel?.invokeMethod("activityToken", arguments: ["id": a.id, "token": hex]) }
          }
        }
        result(["started": true, "id": a.id])
      } catch { result(["started": false, "reason": "\(error)"]) }
    case "updateActivity":
      guard let a = activity ?? Activity<BobixActivityAttributes>.activities.first else { return result(["updated": false]) }
      let s = state(args)
      Task { await a.update(.init(state: s, staleDate: nil)); result(["updated": true]) }
    case "endActivity":
      let all = Activity<BobixActivityAttributes>.activities
      Task {
        for a in all { await a.end(nil, dismissalPolicy: .immediate) }
        self.activity = nil
        result(nil)
      }
    default: result(FlutterMethodNotImplemented)
    }
  }
}
