import Flutter
import UIKit

class SceneDelegate: FlutterSceneDelegate {
  private var push: PushBridge? { (UIApplication.shared.delegate as? AppDelegate)?.push }

  /// A widget, the Live Activity or a control opened the app by a `bobix://` link: a cold start brings it here.
  override func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
    super.scene(scene, willConnectTo: session, options: connectionOptions)
    connectionOptions.urlContexts.forEach { push?.didOpen(url: $0.url) }
  }

  override func scene(_ scene: UIScene, openURLContexts URLContexts: Set<UIOpenURLContext>) {
    super.scene(scene, openURLContexts: URLContexts)
    URLContexts.forEach { push?.didOpen(url: $0.url) }
  }
}
