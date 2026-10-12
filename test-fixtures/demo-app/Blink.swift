import UIKit

/// A control that takes itself away and comes back, over a layer that also
/// takes a touch — the shape of a player's "go live" pill over its picture.
/// A touch aimed at the pill that arrives while it is away reaches the
/// layer; `blink-counts` says which got each one.
final class BlinkController: UIViewController {
  private let counts = UILabel()
  private let under = UIButton(type: .system)
  private let pill = UIButton(type: .system)
  private var pillTaps = 0
  private var underTaps = 0
  private var timer: Timer?

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    counts.accessibilityIdentifier = "blink-counts"
    under.backgroundColor = .darkGray
    under.accessibilityIdentifier = "blink-under"
    under.addAction(UIAction { [weak self] _ in self?.underTaps += 1; self?.show() }, for: .touchUpInside)
    pill.setTitle("GO LIVE", for: .normal)
    pill.backgroundColor = .systemRed
    pill.setTitleColor(.white, for: .normal)
    pill.accessibilityIdentifier = "blink-pill"
    pill.addAction(UIAction { [weak self] _ in self?.pillTaps += 1; self?.show() }, for: .touchUpInside)
    for v in [counts, under, pill] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    NSLayoutConstraint.activate([
      counts.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
      counts.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
      counts.heightAnchor.constraint(equalToConstant: 44),
      under.topAnchor.constraint(equalTo: counts.bottomAnchor, constant: 16),
      under.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
      under.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -16),
      under.heightAnchor.constraint(equalToConstant: 300),
      pill.centerXAnchor.constraint(equalTo: under.centerXAnchor),
      pill.centerYAnchor.constraint(equalTo: under.centerYAnchor),
      pill.widthAnchor.constraint(equalToConstant: 140),
      pill.heightAnchor.constraint(equalToConstant: 56),
    ])
    show()
    timer = Timer.scheduledTimer(withTimeInterval: 0.7, repeats: true) { [weak self] _ in
      guard let self else { return }
      self.pill.isHidden.toggle()
    }
  }

  private func show() {
    counts.text = "pill=\(pillTaps) under=\(underTaps)"
  }
}
