import Flutter
import UIKit

/// The app switcher keeps a picture of the app when it goes away. While the scene is not active an opaque cover (the
/// pet, or the app tile before there is one, on the page colour) hides the content, so nothing of a request, a plan or a session is in that picture.
/// (Dart covers the screen too, but only native code can do it before the system takes the picture.)
class SceneDelegate: FlutterSceneDelegate {
  private var cover: UIView?

  override func sceneWillResignActive(_ scene: UIScene) {
    super.sceneWillResignActive(scene)
    guard cover == nil, let window = (scene as? UIWindowScene)?.windows.first else { return }
    let view = UIView(frame: window.bounds)
    view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    // The page's own colour (the app's `bg` token) with a working pet (or, before there is one, the app tile) on it.
    view.backgroundColor = UIColor { $0.userInterfaceStyle == .dark
      ? UIColor(red: 0.055, green: 0.059, blue: 0.071, alpha: 1)
      : UIColor(red: 0.961, green: 0.965, blue: 0.973, alpha: 1) }
    if let pet = randomPetImage() {
      // A pet picked at random each time, at the Dart cover's size (a still: nothing can animate before the picture).
      let image = UIImageView(image: pet)
      image.contentMode = .scaleAspectFit
      image.frame = CGRect(x: 0, y: 0, width: 96, height: 96)
      image.center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
      image.autoresizingMask = [.flexibleLeftMargin, .flexibleRightMargin, .flexibleTopMargin, .flexibleBottomMargin]
      view.addSubview(image)
    } else {
      let side: CGFloat = 96
      let tile = UIView(frame: CGRect(x: 0, y: 0, width: side, height: side))
      tile.center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
      tile.autoresizingMask = [.flexibleLeftMargin, .flexibleRightMargin, .flexibleTopMargin, .flexibleBottomMargin]
      tile.layer.cornerRadius = 28
      tile.layer.cornerCurve = .continuous
      tile.layer.masksToBounds = true
      let gradient = CAGradientLayer()
      gradient.frame = tile.bounds
      gradient.colors = [
        UIColor(red: 0.96, green: 0.58, blue: 0.43, alpha: 1).cgColor,
        UIColor(red: 0.80, green: 0.28, blue: 0.20, alpha: 1).cgColor,
      ]
      tile.layer.addSublayer(gradient)
      let paw = UIImageView(image: UIImage(systemName: "pawprint",
        withConfiguration: UIImage.SymbolConfiguration(pointSize: 44, weight: .regular)))
      paw.tintColor = .white
      paw.contentMode = .center
      paw.frame = tile.bounds
      tile.addSubview(paw)
      view.addSubview(tile)
    }
    window.addSubview(view)
    cover = view
  }

  override func sceneDidBecomeActive(_ scene: UIScene) {
    cover?.removeFromSuperview()
    cover = nil
    super.sceneDidBecomeActive(scene)
  }

  private func randomPetImage() -> UIImage? {
    let fm = FileManager.default
    var folders: [URL] = []
    if let support = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
      folders.append(support.appendingPathComponent("cover-pets"))
    }
    if let shared = SharedListStore.directory() { folders.append(shared.appendingPathComponent("pets")) }
    return CoverPet.pick(from: folders)
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

/// Which picture the privacy cover shows. The app draws its own stills into `cover-pets` in its support folder the first
/// time it runs (so there is always one), and the pictures it keeps in the App Group for the widgets count too, working
/// pets only: the others are drawn in a session's real state (waiting, failed), which would say something about it. A
/// picture is a pet and nothing else; none at all means the app tile.
enum CoverPet {
  static let suffix = "-working.png"

  static func pick(from folders: [URL]) -> UIImage? {
    let fm = FileManager.default
    let files = folders.flatMap { (try? fm.contentsOfDirectory(at: $0, includingPropertiesForKeys: nil)) ?? [] }
    return files.filter { $0.lastPathComponent.hasSuffix(suffix) }.shuffled().lazy.compactMap { UIImage(contentsOfFile: $0.path) }.first
  }
}
