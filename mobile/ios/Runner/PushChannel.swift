import Flutter
import Foundation
import UIKit
import UserNotifications

/// The platform channel `dev.bobix.phone/push`: the Dart side of push (see lib/push). It asks for permission, registers
/// with APNs directly (no Firebase), hands over the device token, makes the signed grant, reports taps, and holds the
/// shared receive-state file through the push core (`mobile/push-core`) so the app and the notification service
/// extension, which are two processes, never undo each other.
///
/// It exists from launch, before Flutter's engine does, because a tap on a notification can be what launched the app.
final class PushBridge {
  private var channel: FlutterMethodChannel?
  private var token: String?
  /// A tap that arrived before Dart was listening (the app was started by it); Dart takes it once with `initialTap`.
  /// Dart's first `initialTap` call is also its "I am listening" signal: until then no tap goes over the channel, where
  /// nothing would handle it and it would be lost.
  private var pendingTap: [String: String]?
  private var dartListening = false

  /// What the system says about the app's APNs environment: `development` or `production` in the provisioning profile.
  /// A build without a profile is a simulator build (sandbox) or an App Store / TestFlight build (production).
  static func apsEnvironment(profile: Data?) -> String {
    #if targetEnvironment(simulator)
    return "sandbox"
    #else
    guard let profile else { return "production" }
    return environment(inProfile: profile) ?? "production"
    #endif
  }

  /// The `aps-environment` entitlement inside a provisioning profile (a signed plist: the XML sits in the clear).
  static func environment(inProfile data: Data) -> String? {
    let text = String(decoding: data, as: UTF8.self)
    guard let key = text.range(of: "<key>aps-environment</key>"),
      let open = text.range(of: "<string>", range: key.upperBound..<text.endIndex),
      let close = text.range(of: "</string>", range: open.upperBound..<text.endIndex)
    else { return nil }
    switch text[open.upperBound..<close.lowerBound] {
    case "development": return "sandbox"
    case "production": return "production"
    default: return nil
    }
  }

  func attach(messenger: FlutterBinaryMessenger) {
    let c = FlutterMethodChannel(name: "dev.bobix.phone/push", binaryMessenger: messenger)
    c.setMethodCallHandler { [weak self] call, result in self?.handle(call, result) }
    channel = c
  }

  // MARK: events from the system

  func didRegister(deviceToken: Data) {
    let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
    token = hex
    channel?.invokeMethod("token", arguments: hex)
  }

  func didFailToRegister(_ error: Error) {
    channel?.invokeMethod("tokenError", arguments: error.localizedDescription)
  }

  func didTap(_ response: UNNotificationResponse) {
    didTap(userInfo: response.notification.request.content.userInfo)
  }

  func didTap(userInfo: [AnyHashable: Any]) {
    guard let r = PushRoute.parse(userInfo) else { return }
    let tap = ["computer": r.computer, "ref": r.ref, "kind": r.kind]
    if dartListening, let channel { channel.invokeMethod("tap", arguments: tap) } else { pendingTap = tap }
  }

  /// What Dart's `initialTap` takes: the tap that started the app, once. Calling it means Dart now listens for `tap`.
  func takeInitialTap() -> [String: String]? {
    dartListening = true
    defer { pendingTap = nil }
    return pendingTap
  }

  // MARK: calls from Dart

  private func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    let args = call.arguments as? [String: Any] ?? [:]
    func str(_ k: String) -> String { args[k] as? String ?? "" }
    func bytes(_ k: String) -> Data { (args[k] as? FlutterStandardTypedData)?.data ?? Data() }
    func fail(_ e: Error) { result(FlutterError(code: "push_error", message: String(describing: e), details: nil)) }
    guard let path = PushPaths.storePath() else {
      // No App Group (a build without the capability): only the permission calls make sense.
      switch call.method {
      case "permissionStatus", "requestPermission", "register", "token", "environment", "initialTap", "openSettings", "grant": break
      default: return result(FlutterError(code: "no_app_group", message: "the app group is not available", details: nil))
      }
      return handleSystem(call, result, args)
    }
    switch call.method {
    case "setComputer":
      do {
        guard let lo = UInt8(exactly: args["versionMin"] as? Int ?? 1), let hi = UInt8(exactly: args["versionMax"] as? Int ?? 1) else {
          return result(FlutterError(code: "bad_version", message: "the version range is outside 0 to 255", details: nil))
        }
        try storeSetComputer(storePath: path, computerId: str("id"), peerBoxPublic: bytes("boxPublic"), versionMin: lo, versionMax: hi)
        result(nil)
      } catch { fail(error) }
    case "removeComputer":
      do { try storeRemoveComputer(storePath: path, computerId: str("id")); result(nil) } catch { fail(error) }
    case "setHideDetails":
      do { try storeSetHideDetails(storePath: path, hide: args["hide"] as? Bool ?? false); result(nil) } catch { fail(error) }
    case "hideDetails":
      do { result(try storeHideDetails(storePath: path)) } catch { fail(error) }
    case "openPacket":
      guard let box = PushBoxKey.read() else { return result(["error": "noKey"]) }
      do {
        let o = try openPacket(storePath: path, boxSecret: box, packet: bytes("packet"))
        result(["computer": o.computerId, "stream": o.stream, "counter": NSNumber(value: o.counter), "plaintext": FlutterStandardTypedData(bytes: o.plaintext)])
      } catch PushError.Replayed { result(["error": "replayed"]) }
      catch PushError.Version { result(["error": "version"]) }
      catch PushError.Unreadable { result(["error": "unreadable"]) }
      catch { fail(error) }
    default: handleSystem(call, result, args)
    }
  }

  private func handleSystem(_ call: FlutterMethodCall, _ result: @escaping FlutterResult, _ args: [String: Any]) {
    switch call.method {
    case "permissionStatus":
      UNUserNotificationCenter.current().getNotificationSettings { s in
        let v: String
        switch s.authorizationStatus {
        case .notDetermined: v = "notDetermined"
        case .denied: v = "denied"
        default: v = "authorized"
        }
        DispatchQueue.main.async { result(v) }
      }
    case "requestPermission":
      UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
        DispatchQueue.main.async {
          if granted { UIApplication.shared.registerForRemoteNotifications() }
          result(granted)
        }
      }
    case "register":
      // Safe to repeat: APNs hands back the same token, and a new one when the system changed it.
      UIApplication.shared.registerForRemoteNotifications()
      result(nil)
    case "grant":
      func str(_ k: String) -> String { args[k] as? String ?? "" }
      // `auth` never leaves this process: Dart gets the signed grant, not the key.
      guard let store = try? KeyStore(), let auth = (try? store.secret("auth")) ?? nil, let box = (try? store.secret("box")) ?? nil else {
        return result(FlutterError(code: "no_key", message: "the device keys are not there", details: nil))
      }
      do {
        let g = try makeGrant(authSeed: auth, boxSecret: box, token: str("token"), relayId: str("relayId"), expiresUnixMs: UInt64(exactly: args["expiresMs"] as? Int ?? 0) ?? 0)
        result(g)
      } catch { result(FlutterError(code: "push_error", message: String(describing: error), details: nil)) }
    case "token": result(token)
    case "environment":
      let url = Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision")
      result(PushBridge.apsEnvironment(profile: url.flatMap { try? Data(contentsOf: $0) }))
    case "initialTap": result(takeInitialTap())
    case "openSettings":
      if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
      result(nil)
    default: result(FlutterMethodNotImplemented)
    }
  }
}
