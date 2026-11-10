import UIKit

/// `-playing`: a picture redrawn every frame across the top half — a live
/// video, as a consumer's home screen has one — and below it a still half
/// with a button that counts its presses in `playing-count`.
///
/// A point touch waits for the screen to hold still, and a screen with a
/// video on it never does: every touch here waited out the whole limit.
final class PlayingController: UIViewController {
  private let picture = UIView()
  private var link: CADisplayLink?
  private var frame = 0
  private var presses = 0
  private let count = UILabel()

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    picture.isAccessibilityElement = true
    picture.accessibilityIdentifier = "playing-picture"
    count.text = "pressed 0"
    count.accessibilityIdentifier = "playing-count"
    let press = UIButton(type: .system)
    press.setTitle("Press", for: .normal)
    press.titleLabel?.font = .preferredFont(forTextStyle: .title1)
    press.accessibilityIdentifier = "playing-press"
    press.addAction(UIAction { [weak self] _ in
      guard let self else { return }
      self.presses += 1
      self.count.text = "pressed \(self.presses)"
    }, for: .touchUpInside)
    for v in [picture, count, press] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    NSLayoutConstraint.activate([
      picture.topAnchor.constraint(equalTo: view.topAnchor),
      picture.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      picture.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      picture.heightAnchor.constraint(equalTo: view.heightAnchor, multiplier: 0.45),
      press.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      count.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      count.topAnchor.constraint(equalTo: press.bottomAnchor, constant: 24),
    ])
    // the button at 75% down, wherever the screen's height puts that
    view.addConstraint(NSLayoutConstraint(
      item: press, attribute: .centerY, relatedBy: .equal,
      toItem: view, attribute: .bottom, multiplier: 0.75, constant: 0))
  }

  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    link = CADisplayLink(target: self, selector: #selector(redraw))
    link?.add(to: .main, forMode: .common)
  }

  override func viewWillDisappear(_ animated: Bool) {
    super.viewWillDisappear(animated)
    link?.invalidate()
    link = nil
  }

  @objc private func redraw() {
    frame += 1
    // a different level each frame, far apart, as a video's pixels are;
    // a slow sweep through hues has stretches where the brightness barely
    // moves, and two screenshots there read as still
    picture.backgroundColor = UIColor(white: CGFloat(30 + (frame * 97) % 211) / 255, alpha: 1)
  }
}
