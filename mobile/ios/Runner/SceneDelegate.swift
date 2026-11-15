import Flutter
import UIKit

/// The app switcher keeps a picture of the app when it goes away. While the scene is not active a plain cover hides the
/// content, so nothing of a request, a plan or a session is in that picture. (Dart covers the screen too, but only native
/// code can do it before the system takes the picture.)
class SceneDelegate: FlutterSceneDelegate {
  private var cover: UIView?

  override func sceneWillResignActive(_ scene: UIScene) {
    super.sceneWillResignActive(scene)
    guard cover == nil, let window = (scene as? UIWindowScene)?.windows.first else { return }
    let view = UIView(frame: window.bounds)
    view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    view.backgroundColor = .systemBackground
    let label = UILabel()
    label.text = "Bobix"
    label.font = .systemFont(ofSize: 28, weight: .bold)
    label.textColor = .secondaryLabel
    label.sizeToFit()
    label.center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
    label.autoresizingMask = [.flexibleLeftMargin, .flexibleRightMargin, .flexibleTopMargin, .flexibleBottomMargin]
    view.addSubview(label)
    window.addSubview(view)
    cover = view
  }

  override func sceneDidBecomeActive(_ scene: UIScene) {
    cover?.removeFromSuperview()
    cover = nil
    super.sceneDidBecomeActive(scene)
  }

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
