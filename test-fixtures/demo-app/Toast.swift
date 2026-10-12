import SwiftUI
import UIKit

/// A toast in the shape a consumer's app shows one: a card added to the
/// current view controller's view, named `toast`, its words in a plain
/// label, closed by a button named `toast-close`. With `-toast-passthrough`
/// the card answers `point(inside:)` only on its close button, so touches
/// elsewhere go to what is under it — the consumer's card does. With
/// `-toast-animated` it fades in and out as theirs does.
final class ToastController: UIViewController {
  /// Where the card goes: this view, a SwiftUI hosting view
  /// (`-toast-hosting`), or a view that lists its own accessibility
  /// elements (`-toast-elements`).
  private var host: UIView!

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    host = view
    if CommandLine.arguments.contains("-toast-hosting") {
      let hosting = UIHostingController(rootView: Text("a SwiftUI screen").accessibilityIdentifier("toast-screen"))
      addChild(hosting)
      hosting.view.frame = view.bounds
      hosting.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      view.addSubview(hosting.view)
      hosting.didMove(toParent: self)
      host = hosting.view
    } else if CommandLine.arguments.contains("-toast-elements") {
      let listed = UIView(frame: view.bounds)
      listed.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      let title = UILabel(frame: CGRect(x: 20, y: 120, width: 300, height: 40))
      title.text = "a listed screen"
      listed.addSubview(title)
      listed.accessibilityElements = [title]
      view.addSubview(listed)
      host = listed
    }
    let show = UIButton(type: .system)
    show.setTitle("Save screenshot", for: .normal)
    show.accessibilityIdentifier = "toast-show"
    show.addAction(UIAction { [weak self] _ in self?.toast("Screenshot saved") }, for: .touchUpInside)
    show.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(show)
    NSLayoutConstraint.activate([
      show.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      show.centerYAnchor.constraint(equalTo: view.centerYAnchor),
    ])
  }

  private func toast(_ message: String) {
    let card: UIView = CommandLine.arguments.contains("-toast-passthrough") ? PassThroughCard() : UIView()
    card.backgroundColor = .darkGray
    card.layer.cornerRadius = 12
    card.accessibilityIdentifier = "toast"
    let words = UILabel()
    words.text = message
    words.textColor = .white
    let close = UIButton(type: .system)
    close.setTitle("×", for: .normal)
    close.accessibilityIdentifier = "toast-close"
    close.accessibilityLabel = "Close"
    close.addAction(UIAction { _ in card.removeFromSuperview() }, for: .touchUpInside)
    (card as? PassThroughCard)?.close = close
    for v in [words, close] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      card.addSubview(v)
    }
    card.translatesAutoresizingMaskIntoConstraints = false
    host.addSubview(card)
    NSLayoutConstraint.activate([
      card.topAnchor.constraint(equalTo: host.safeAreaLayoutGuide.topAnchor, constant: 16),
      card.leadingAnchor.constraint(equalTo: host.leadingAnchor, constant: 20),
      card.trailingAnchor.constraint(equalTo: host.trailingAnchor, constant: -20),
      card.heightAnchor.constraint(equalToConstant: 56),
      words.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 16),
      words.centerYAnchor.constraint(equalTo: card.centerYAnchor),
      close.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -12),
      close.centerYAnchor.constraint(equalTo: card.centerYAnchor),
    ])
    NSLog("[ToastController] said: \(message)")
    if CommandLine.arguments.contains("-toast-animated") {
      // The consumer's way in and out, with their out stretched from
      // 2.5 s to 8 so a reader has time to look: the fade-out is
      // scheduled the moment the card appears, which sets its alpha to 0
      // then, while it stays on screen until the delay runs out.
      card.alpha = 0
      UIView.animate(withDuration: 0.2) { card.alpha = 1 }
      UIView.animate(
        withDuration: 0.15, delay: 8, options: [], animations: { card.alpha = 0 },
        completion: { _ in card.removeFromSuperview() })
    } else {
      DispatchQueue.main.asyncAfter(deadline: .now() + 8) { card.removeFromSuperview() }
    }
  }
}

private final class PassThroughCard: UIView {
  weak var close: UIView?
  override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
    guard let close else { return false }
    return close.frame.contains(point)
  }
}
