import UIKit

/// An Apple TV screen to drive: three buttons in a row, and a label that
/// says which was selected last and how many times in all
/// (`tv-count`: "pressed middle 2"). A television has no touch, so every
/// press arrives through the focus engine and the remote's select.
final class TVController: UIViewController {
  private let count = UILabel()
  private var presses = 0

  override func viewDidLoad() {
    super.viewDidLoad()
    count.text = "pressed none 0"
    count.accessibilityIdentifier = "tv-count"
    count.font = .preferredFont(forTextStyle: .title2)
    let row = UIStackView()
    row.axis = .horizontal
    row.spacing = 80
    for name in ["left", "middle", "right"] {
      let b = UIButton(type: .system)
      b.setTitle(name.capitalized, for: .normal)
      b.accessibilityIdentifier = "tv-\(name)"
      b.addAction(UIAction { [weak self] _ in
        guard let self else { return }
        self.presses += 1
        self.count.text = "pressed \(name) \(self.presses)"
      }, for: .primaryActionTriggered)
      row.addArrangedSubview(b)
    }
    let column = UIStackView(arrangedSubviews: [row, count])
    column.axis = .vertical
    column.alignment = .center
    column.spacing = 60
    column.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(column)
    NSLayoutConstraint.activate([
      column.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      column.centerYAnchor.constraint(equalTo: view.centerYAnchor),
    ])
  }
}

final class AppDelegate: UIResponder, UIApplicationDelegate {
  func application(
    _ application: UIApplication,
    configurationForConnecting connectingSceneSession: UISceneSession,
    options: UIScene.ConnectionOptions
  ) -> UISceneConfiguration {
    let config = UISceneConfiguration(name: "Default", sessionRole: connectingSceneSession.role)
    config.delegateClass = SceneDelegate.self
    return config
  }
}

/// tvOS 27 ends an app that makes its window without a scene, before its
/// first frame.
final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?

  func scene(
    _ scene: UIScene, willConnectTo session: UISceneSession,
    options connectionOptions: UIScene.ConnectionOptions
  ) {
    guard let windowScene = scene as? UIWindowScene else { return }
    let window = UIWindow(windowScene: windowScene)
    window.rootViewController = TVController()
    window.makeKeyAndVisible()
    self.window = window
  }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))
