import UIKit

// This app is built with Flutter 3.32, whose iOS embedder ships the classic
// (pre-UIScene) launch path. The AppDelegate + Main.storyboard flow is used
// and Info.plist defines no UIApplicationSceneManifest, so this scene delegate
// is never instantiated. It is kept as a compilable placeholder because it is
// referenced from the Runner target's Sources build phase.
class SceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?
}
