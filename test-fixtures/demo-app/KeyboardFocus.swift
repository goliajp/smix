import UIKit

/// Three controls in a column for a hardware keyboard to reach: a button,
/// a plain view that acts on a tap gesture (not a control), and another
/// button. `kf_result` counts what each one received, so a gate reads from
/// the app whether a key activated it.
final class KeyboardFocusController: UIViewController {
  private let result = UILabel()
  private var counts = ["first": 0, "shared": 0, "last": 0]

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    result.accessibilityIdentifier = "kf_result"
    show()

    let first = button("first")
    let shared = UIView()
    shared.backgroundColor = .systemGray5
    shared.isAccessibilityElement = true
    shared.accessibilityLabel = "shared"
    shared.accessibilityIdentifier = "kf_shared"
    shared.accessibilityTraits = .button
    shared.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(sharedTapped)))
    let sharedLabel = UILabel()
    sharedLabel.text = "shared"
    sharedLabel.translatesAutoresizingMaskIntoConstraints = false
    shared.addSubview(sharedLabel)
    let last = button("last")

    let column = UIStackView(arrangedSubviews: [result, first, shared, last])
    column.axis = .vertical
    column.spacing = 16
    column.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(column)
    NSLayoutConstraint.activate([
      column.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 20),
      column.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
      column.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
      shared.heightAnchor.constraint(equalToConstant: 44),
      sharedLabel.centerYAnchor.constraint(equalTo: shared.centerYAnchor),
      sharedLabel.leadingAnchor.constraint(equalTo: shared.leadingAnchor, constant: 8),
    ])
  }

  private func button(_ name: String) -> UIButton {
    let b = UIButton(type: .system)
    b.setTitle(name, for: .normal)
    b.accessibilityIdentifier = "kf_\(name)"
    b.addAction(UIAction { [weak self] _ in self?.bump(name) }, for: .primaryActionTriggered)
    return b
  }

  @objc private func sharedTapped() { bump("shared") }

  private func bump(_ name: String) {
    counts[name, default: 0] += 1
    show()
  }

  private func show() {
    result.text = "first \(counts["first"]!) shared \(counts["shared"]!) last \(counts["last"]!)"
  }
}
