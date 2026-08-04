import Flutter
import UIKit

class SceneDelegate: FlutterSceneDelegate {

  /// A link the app was launched by.
  ///
  /// With scenes in play this is where a URL arrives — `application(_:open:)`
  /// on the app delegate is not called at all — and on a cold start it comes
  /// through the connect options rather than through `openURLContexts`.
  override func scene(
    _ scene: UIScene,
    willConnectTo session: UISceneSession,
    options connectionOptions: UIScene.ConnectionOptions
  ) {
    super.scene(scene, willConnectTo: session, options: connectionOptions)
    for context in connectionOptions.urlContexts where DeepLinks.shared.handle(context.url) {
      break
    }
  }

  /// A link arriving while the app is already running.
  ///
  /// Anything that is not ours is passed up rather than swallowed: the Flutter
  /// scene delegate hands URLs to registered plugins, and a share extension or
  /// an OAuth callback added later would otherwise stop working for no visible
  /// reason.
  override func scene(_ scene: UIScene, openURLContexts urlContexts: Set<UIOpenURLContext>) {
    let ours = urlContexts.filter { $0.url.scheme?.lowercased() == "mumble" }
    for context in ours {
      DeepLinks.shared.handle(context.url)
    }
    let rest = urlContexts.subtracting(ours)
    if !rest.isEmpty {
      super.scene(scene, openURLContexts: rest)
    }
  }
}
