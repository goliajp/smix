import UIKit

/// A picture that zooms with two fingers and pans once zoomed, with a
/// button that puts it back — the shape of a player's zoom. `zoom-state`
/// says the scale and where the visible part starts.
final class ZoomController: UIViewController, UIScrollViewDelegate {
  private let state = UILabel()
  private let reset = UIButton(type: .system)
  private let surface = UIScrollView()
  private let picture = UIView()

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    state.accessibilityIdentifier = "zoom-state"
    reset.setTitle("Reset Zoom", for: .normal)
    reset.accessibilityIdentifier = "zoom-reset"
    reset.addAction(UIAction { [weak self] _ in self?.surface.setZoomScale(1, animated: false); self?.show() }, for: .touchUpInside)
    surface.accessibilityIdentifier = "zoom-surface"
    surface.minimumZoomScale = 1
    surface.maximumZoomScale = 4
    surface.bouncesZoom = false
    surface.delegate = self
    surface.backgroundColor = .black
    picture.backgroundColor = .systemTeal
    for v in [state, reset, surface] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    surface.addSubview(picture)
    NSLayoutConstraint.activate([
      state.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
      state.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
      state.heightAnchor.constraint(equalToConstant: 44),
      reset.topAnchor.constraint(equalTo: state.bottomAnchor),
      reset.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
      reset.heightAnchor.constraint(equalToConstant: 44),
      surface.topAnchor.constraint(equalTo: reset.bottomAnchor, constant: 16),
      surface.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      surface.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      surface.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
    ])
    show()
  }

  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    if surface.zoomScale == 1 {
      picture.frame = CGRect(origin: .zero, size: surface.bounds.size)
      surface.contentSize = surface.bounds.size
    }
  }

  func viewForZooming(in scrollView: UIScrollView) -> UIView? { picture }
  func scrollViewDidZoom(_ scrollView: UIScrollView) { show() }
  func scrollViewDidScroll(_ scrollView: UIScrollView) { show() }

  private func show() {
    state.text = String(
      format: "scale=%.2f x=%.0f y=%.0f",
      surface.zoomScale, surface.contentOffset.x, surface.contentOffset.y)
  }
}
