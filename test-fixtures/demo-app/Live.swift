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
  private let menu = UIButton(type: .system)
  private let present = UIButton(type: .system)
  private let field = UITextField()
  private let secret = UITextField()
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
    field.placeholder = "type here"
    field.backgroundColor = .white
    field.accessibilityIdentifier = "live-input"
    field.returnKeyType = .default
    field.delegate = self
    secret.placeholder = "password"
    secret.isSecureTextEntry = true
    secret.backgroundColor = .white
    secret.accessibilityIdentifier = "live-password"
    let stack = UIStackView(arrangedSubviews: [counts, menu, present, field, secret])
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
