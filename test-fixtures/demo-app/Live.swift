import UIKit

/// Controls over a picture that never stops moving — the shape of a
/// consumer's screens, which sit over a live video view redrawn every frame.
/// Their reports — a tap accepted and nothing happening, a keyboard that
/// would not go away — came from those screens, and every screen the
/// fixture had was still behind its controls. What each touch reached is
/// counted in `live-counts`, by the controls' own handlers.
final class LiveController: UIViewController, UITextFieldDelegate {
  private let picture = UIView()
  private var link: CADisplayLink?
  private var frame = 0
  private let counts = UILabel()
  private var menu = UIButton(type: .system)
  private let present = UIButton(type: .system)
  private let field = UITextField()
  private let secret = UITextField()
  private let stubborn = UIButton(type: .system)
  private let stubbornCount = UILabel()
  private var stubbornTouches = 0
  private var menuTaps = 0
  private var modalTaps = 0
  private var submits = 0

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .black
    picture.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(picture)
    counts.accessibilityIdentifier = "live-counts"
    counts.textColor = .white
    menu.setTitle("Menu", for: .normal)
    menu.accessibilityIdentifier = "live-menu"
    menu.addAction(UIAction { [weak self] _ in self?.menuTaps += 1; self?.show() }, for: .touchUpInside)
    present.setTitle("Present", for: .normal)
    present.accessibilityIdentifier = "live-present"
    present.addAction(UIAction { [weak self] _ in self?.presentSheet() }, for: .touchUpInside)
    // A control that takes its first touch and does nothing with it, as a
    // control over a live picture sometimes does; the second one counts.
    stubborn.setTitle("Stubborn", for: .normal)
    stubborn.accessibilityIdentifier = "live-stubborn"
    stubborn.addAction(UIAction { [weak self] _ in
      guard let self else { return }
      self.stubbornTouches += 1
      self.stubbornCount.text = "stubborn \(max(0, self.stubbornTouches - 1))"
    }, for: .touchUpInside)
    stubbornCount.text = "stubborn 0"
    stubbornCount.textColor = .white
    stubbornCount.accessibilityIdentifier = "live-stubborn-count"
    field.placeholder = "type here"
    field.backgroundColor = .white
    field.accessibilityIdentifier = "live-input"
    field.returnKeyType = .default
    field.delegate = self
    secret.placeholder = "password"
    secret.isSecureTextEntry = true
    secret.backgroundColor = .white
    secret.accessibilityIdentifier = "live-password"
    let stack = UIStackView(arrangedSubviews: [counts, menu, present, stubborn, stubbornCount, field, secret])
    stack.axis = .vertical
    stack.spacing = 16
    stack.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(stack)
    NSLayoutConstraint.activate([
      picture.topAnchor.constraint(equalTo: view.topAnchor),
      picture.bottomAnchor.constraint(equalTo: view.bottomAnchor),
      picture.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      picture.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 16),
      stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
      stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -16),
      field.heightAnchor.constraint(equalToConstant: 44),
      secret.heightAnchor.constraint(equalToConstant: 44),
    ])
    show()
    // `-slow-start`: what a consumer's app does as it comes up — the
    // first screen is drawn, then the main thread is busy restoring a
    // session, then the controls are built again. A tap in that window
    // is what their flows padded with fixed waits.
    if CommandLine.arguments.contains("-slow-start") {
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
        Thread.sleep(forTimeInterval: 1.5)
        self?.rebuildMenu()
      }
    }
    let link = CADisplayLink(target: self, selector: #selector(redraw))
    link.add(to: .main, forMode: .common)
    self.link = link
  }

  /// Every frame, as a video view repaints.
  @objc private func redraw() {
    frame += 1
    let hue = CGFloat(frame % 240) / 240
    picture.backgroundColor = UIColor(hue: hue, saturation: 0.6, brightness: 0.35, alpha: 1)
  }

  /// The menu button made again, as a screen rebuilt after a restore
  /// makes its controls again: same id, same place, a new object.
  private func rebuildMenu() {
    guard let stack = menu.superview as? UIStackView,
      let at = stack.arrangedSubviews.firstIndex(of: menu)
    else { return }
    let fresh = UIButton(type: .system)
    fresh.setTitle("Menu", for: .normal)
    fresh.accessibilityIdentifier = "live-menu"
    fresh.addAction(UIAction { [weak self] _ in self?.menuTaps += 1; self?.show() }, for: .touchUpInside)
    stack.removeArrangedSubview(menu)
    menu.removeFromSuperview()
    stack.insertArrangedSubview(fresh, at: at)
    menu = fresh
  }

  private func show() {
    counts.text = "menu \(menuTaps) modal \(modalTaps) submit \(submits)"
  }

  private func presentSheet() {
    let sheet = UIViewController()
    sheet.view.backgroundColor = .systemBackground
    let button = UIButton(type: .system)
    button.setTitle("Confirm", for: .normal)
    button.accessibilityIdentifier = "live-modal-confirm"
    button.translatesAutoresizingMaskIntoConstraints = false
    button.addAction(UIAction { [weak self, weak sheet] _ in
      self?.modalTaps += 1
      self?.show()
      sheet?.dismiss(animated: true)
    }, for: .touchUpInside)
    sheet.view.addSubview(button)
    NSLayoutConstraint.activate([
      button.centerXAnchor.constraint(equalTo: sheet.view.centerXAnchor),
      button.centerYAnchor.constraint(equalTo: sheet.view.centerYAnchor),
    ])
    present(sheet, animated: true)
  }

  func textFieldShouldReturn(_ textField: UITextField) -> Bool {
    submits += 1
    show()
    textField.resignFirstResponder()
    return true
  }
}
