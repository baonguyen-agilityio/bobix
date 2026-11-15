import Flutter
import UIKit
import UserNotifications

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  private var secureKeys: SecureKeysChannel?
  let push = PushBridge()
  private let surfaces = SurfacesBridge()

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    // A reinstall must not bring an old key back: erase before anything can read one.
    _ = try? KeyStore()
    // Set before launch finishes: a tap on a notification can be what starts the app.
    UNUserNotificationCenter.current().delegate = self
    #if DEBUG
    PushDebug.run()
    #endif
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    secureKeys = SecureKeysChannel(messenger: engineBridge.applicationRegistrar.messenger())
    push.attach(messenger: engineBridge.applicationRegistrar.messenger())
    surfaces.attach(messenger: engineBridge.applicationRegistrar.messenger())
  }

  override func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
    push.didRegister(deviceToken: deviceToken)
    #if DEBUG
    PushDebug.recordToken(deviceToken.map { String(format: "%02x", $0) }.joined())
    #endif
    super.application(application, didRegisterForRemoteNotificationsWithDeviceToken: deviceToken)
  }

  override func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
    push.didFailToRegister(error)
    #if DEBUG
    PushDebug.recordFailure(error)
    #endif
    super.application(application, didFailToRegisterForRemoteNotificationsWithError: error)
  }

  /// A push without an alert (`closed` is sent that way) wakes the app when iOS allows it, which is not always: then the
  /// surfaces dim by age instead.
  override func application(
    _ application: UIApplication, didReceiveRemoteNotification userInfo: [AnyHashable: Any],
    fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void
  ) {
    completionHandler(SurfacesBridge.applyPush(userInfo) ? .newData : .noData)
  }

  /// A notification that arrives while the app is open still shows: the person may be on another screen.
  override func userNotificationCenter(
    _ center: UNUserNotificationCenter, willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    completionHandler([.banner, .list, .sound])
  }

  override func userNotificationCenter(
    _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    push.didTap(response)
    completionHandler()
  }
}
