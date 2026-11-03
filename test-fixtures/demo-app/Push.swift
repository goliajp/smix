import UIKit
import UserNotifications

/// What a notification does to the app, read from the app's side. The
/// delegate is set before launch finishes, so a tap that launches the app
/// is seen too. In front, a notification shows as a banner as it would in
/// the background — the way a consumer's app presents its own.
final class PushReceiver: NSObject, UNUserNotificationCenterDelegate {
  static let shared = PushReceiver()
  static let opened = Notification.Name("smix.fixture.push-opened")
  /// What the last tapped notification carried: its title and `route`.
  private(set) var lastOpened = "nothing opened"

  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler done: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    done([.banner, .list])
  }

  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler done: @escaping () -> Void
  ) {
    let content = response.notification.request.content
    let route = content.userInfo["route"] as? String ?? "none"
    DispatchQueue.main.async {
      self.lastOpened = "opened \(content.title) → \(route)"
      NotificationCenter.default.post(name: Self.opened, object: nil)
    }
    done()
  }
}

/// `-push`: a button that asks for notifications, what the system says
/// about them, and what the last tapped notification opened.
final class PushController: UIViewController {
  private let permission = UILabel()
  private let opened = UILabel()

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    let ask = UIButton(type: .system)
    ask.setTitle("Ask notifications", for: .normal)
    ask.accessibilityIdentifier = "push-ask"
    ask.addAction(UIAction { [weak self] _ in
      UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in
        self?.readPermission()
      }
    }, for: .touchUpInside)
    permission.accessibilityIdentifier = "push-permission"
    opened.accessibilityIdentifier = "push-opened"
    opened.text = PushReceiver.shared.lastOpened
    let stack = UIStackView(arrangedSubviews: [ask, permission, opened])
    stack.axis = .vertical
    stack.spacing = 16
    stack.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(stack)
    NSLayoutConstraint.activate([
      stack.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      stack.centerYAnchor.constraint(equalTo: view.centerYAnchor),
    ])
    NotificationCenter.default.addObserver(
      forName: PushReceiver.opened, object: nil, queue: .main
    ) { [weak self] _ in
      self?.opened.text = PushReceiver.shared.lastOpened
    }
    readPermission()
  }

  private func readPermission() {
    UNUserNotificationCenter.current().getNotificationSettings { settings in
      let said = settings.authorizationStatus == .authorized ? "allowed" : "not allowed"
      DispatchQueue.main.async { self.permission.text = "notifications \(said)" }
    }
  }
}
