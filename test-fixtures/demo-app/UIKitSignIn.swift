import UIKit

/// A UIKit sign-in step in the shape of a consumer's: the field sits in a
/// scroll view that dismisses the keyboard interactively, the Continue
/// button is a plain view with a tap recognizer in a bar pinned to the
/// keyboard layout guide, and the field's return key is Go, which submits
/// like the button does. The field is a sign-in name, so the
/// keyboard carries a Passwords bar above its keys.
///
/// `uks_result` is the app's own count of submissions, and from which
/// way each came.
///
/// Nothing on the screen takes the keyboard down unless `tapsAway`, which
/// lets a touch on an empty spot end editing, as many forms do.
final class UIKitSignInController: UIViewController, UITextFieldDelegate {
  private let tapsAway: Bool
  private let scroll = UIScrollView()
  private let field = UITextField()
  private let result = UILabel()
  private let bar = UIStackView()
  private let button = UIView()
  private var submits: [String] = []

  init(tapsAway: Bool) {
    self.tapsAway = tapsAway
    super.init(nibName: nil, bundle: nil)
  }

  required init?(coder: NSCoder) { fatalError("not from a storyboard") }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    // content shorter than the scroll view, so a drag does not scroll it
    // and cannot take the keyboard down
    scroll.keyboardDismissMode = .interactive
    scroll.accessibilityIdentifier = "uks_scroll"

    result.text = "submitted 0"
    result.accessibilityIdentifier = "uks_result"
    field.placeholder = "email"
    field.borderStyle = .roundedRect
    field.autocapitalizationType = .none
    field.keyboardType = .emailAddress
    // a sign-in name: the keyboard grows a Passwords bar above its keys
    field.textContentType = .username
    // a return key named for its action, which submits: hideKeyboard must
    // not press it
    field.returnKeyType = .go
    field.delegate = self
    field.accessibilityIdentifier = "uks_email"

    let body = UIStackView(arrangedSubviews: [result, field])
    body.axis = .vertical
    body.spacing = 16
    body.translatesAutoresizingMaskIntoConstraints = false
    scroll.addSubview(body)

    let title = UILabel()
    title.text = "Continue"
    title.textColor = .white
    title.textAlignment = .center
    title.translatesAutoresizingMaskIntoConstraints = false
    button.backgroundColor = .systemBlue
    button.addSubview(title)
    button.isAccessibilityElement = true
    button.accessibilityTraits = .button
    button.accessibilityLabel = "Continue"
    button.accessibilityIdentifier = "uks_continue"
    button.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped)))
    if tapsAway {
      let away = UITapGestureRecognizer(target: view, action: #selector(UIView.endEditing(_:)))
      away.cancelsTouchesInView = false
      view.addGestureRecognizer(away)
    }
    bar.addArrangedSubview(button)

    for v in [scroll, bar] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    NSLayoutConstraint.activate([
      scroll.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
      scroll.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      scroll.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      scroll.bottomAnchor.constraint(equalTo: bar.topAnchor),
      body.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor, constant: 16),
      body.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor),
      body.leadingAnchor.constraint(equalTo: scroll.frameLayoutGuide.leadingAnchor, constant: 18),
      body.trailingAnchor.constraint(equalTo: scroll.frameLayoutGuide.trailingAnchor, constant: -18),
      bar.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 18),
      bar.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -18),
      bar.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor),
      button.heightAnchor.constraint(equalToConstant: 44),
      title.centerXAnchor.constraint(equalTo: button.centerXAnchor),
      title.centerYAnchor.constraint(equalTo: button.centerYAnchor),
    ])
  }

  private func submit(_ how: String) {
    view.endEditing(true)
    submits.append(how)
    result.text = "submitted \(submits.count): \(submits.joined(separator: ","))"
  }

  @objc private func tapped() { submit("button") }

  func textFieldShouldReturn(_ textField: UITextField) -> Bool {
    submit("return")
    return false
  }
}
