import UIKit

/// `-arrive`: a screen whose contents slide in over 1.5 s after it is
/// pushed, and a button on it that counts its presses.
///
/// A touch sent at the button's place as soon as the screen was pushed
/// lands where the button is not yet, and nothing happens — on a
/// consumer's screen that took that long to arrive, a touch answered
/// `tapped` five times in a row and did nothing. `arrive-count` says
/// whether the press got through.
final class ArriveController: UINavigationController {
  init() {
    super.init(rootViewController: ArriveStartController())
  }

  required init?(coder: NSCoder) { fatalError("not from a storyboard") }
}

private final class ArriveStartController: UIViewController {
  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    title = "Start"
    let open = UIButton(type: .system)
    open.setTitle("Open slowly", for: .normal)
    open.accessibilityIdentifier = "arrive-open"
    open.addAction(UIAction { [weak self] _ in
      self?.navigationController?.pushViewController(ArrivedController(), animated: true)
    }, for: .touchUpInside)
    pin(open, in: view)
  }
}

private final class ArrivedController: UIViewController {
  private var presses = 0
  private let count = UILabel()
  private var press: UIButton?

  override func viewWillAppear(_ animated: Bool) {
    super.viewWillAppear(animated)
    let away = CGAffineTransform(translationX: view.bounds.width, y: 0)
    press?.transform = away
    count.transform = away
  }

  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    UIView.animate(withDuration: 1.5, delay: 0, options: [.curveLinear, .allowUserInteraction]) {
      self.press?.transform = .identity
      self.count.transform = .identity
    }
  }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    title = "Arrived"
    let press = UIButton(type: .system)
    press.setTitle("Press", for: .normal)
    press.accessibilityIdentifier = "arrive-press"
    press.addAction(UIAction { [weak self] _ in
      guard let self else { return }
      self.presses += 1
      self.count.text = "pressed \(self.presses)"
    }, for: .touchUpInside)
    count.text = "pressed 0"
    count.accessibilityIdentifier = "arrive-count"
    pin(press, in: view)
    self.press = press
    count.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(count)
    NSLayoutConstraint.activate([
      count.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      count.topAnchor.constraint(equalTo: press.bottomAnchor, constant: 24),
    ])
  }
}

/// The same place on both screens, so a point aimed at the start
/// screen's button is the arrived screen's once it is in.
private func pin(_ button: UIButton, in view: UIView) {
  button.translatesAutoresizingMaskIntoConstraints = false
  view.addSubview(button)
  NSLayoutConstraint.activate([
    button.centerXAnchor.constraint(equalTo: view.centerXAnchor),
    button.centerYAnchor.constraint(equalTo: view.centerYAnchor),
    button.widthAnchor.constraint(equalToConstant: 200),
    button.heightAnchor.constraint(equalToConstant: 60),
  ])
}
