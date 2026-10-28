import UIKit

/// A zoom read straight off a `UIPinchGestureRecognizer`: the zoom when
/// the pinch began times the recognizer's `scale`, the way a consumer's
/// player computes it. `pinch-state` says the zoom.
final class PinchController: UIViewController {
  private let state = UILabel()
  private let surface = UIView()
  private var zoom: CGFloat = 1
  private var startZoom: CGFloat = 1

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    state.accessibilityIdentifier = "pinch-state"
    surface.accessibilityIdentifier = "pinch-surface"
    surface.backgroundColor = .darkGray
    surface.addGestureRecognizer(UIPinchGestureRecognizer(target: self, action: #selector(pinched(_:))))
    for v in [state, surface] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    NSLayoutConstraint.activate([
      state.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
      state.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
      state.heightAnchor.constraint(equalToConstant: 44),
      surface.topAnchor.constraint(equalTo: state.bottomAnchor, constant: 16),
      surface.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      surface.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      surface.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
    ])
    show()
  }

  @objc private func pinched(_ r: UIPinchGestureRecognizer) {
    switch r.state {
    case .began: startZoom = zoom
    case .changed, .ended: zoom = min(5, max(1, startZoom * r.scale))
    default: break
    }
    show()
  }

  private func show() {
    state.text = String(format: "scale=%.2f", zoom)
  }
}
