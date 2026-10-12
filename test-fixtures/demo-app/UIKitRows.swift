import UIKit

/// A UIKit table taller than the screen, each row a button with swipe
/// actions, in the shape of a consumer's rule list.
///
/// UIKit lists every row of a table to accessibility, on screen or not,
/// and iOS 27 places the controls inside an off-screen row at the
/// table's origin. `uk_result` is the app's own reading of which row a
/// touch reached and which row's Edit was pressed.
final class UIKitRowsController: UIViewController, UITableViewDataSource, UITableViewDelegate {
  private let result = UILabel()
  private let table = UITableView(frame: .zero, style: .plain)

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    result.text = "nothing yet"
    result.accessibilityIdentifier = "uk_result"
    table.dataSource = self
    table.delegate = self
    table.rowHeight = 66
    table.register(UITableViewCell.self, forCellReuseIdentifier: "row")
    for v in [result, table] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    NSLayoutConstraint.activate([
      result.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
      result.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
      result.heightAnchor.constraint(equalToConstant: 44),
      table.topAnchor.constraint(equalTo: result.bottomAnchor),
      table.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      table.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      table.bottomAnchor.constraint(equalTo: view.bottomAnchor),
    ])
  }

  func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { 30 }

  func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
    let cell = tableView.dequeueReusableCell(withIdentifier: "row", for: indexPath)
    cell.contentView.subviews.forEach { $0.removeFromSuperview() }
    let i = indexPath.row
    let button = UIButton(type: .system)
    button.setTitle("Rule \(i)", for: .normal)
    button.contentHorizontalAlignment = .leading
    button.accessibilityIdentifier = "uk_row_\(i)"
    button.addAction(UIAction { [weak self] _ in self?.result.text = "opened \(i)" }, for: .touchUpInside)
    button.frame = cell.contentView.bounds.insetBy(dx: 16, dy: 0)
    button.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    cell.contentView.addSubview(button)
    return cell
  }

  func tableView(
    _ tableView: UITableView,
    trailingSwipeActionsConfigurationForRowAt indexPath: IndexPath
  ) -> UISwipeActionsConfiguration? {
    let i = indexPath.row
    let edit = UIContextualAction(style: .normal, title: "Edit") { [weak self] _, _, done in
      self?.result.text = "edited \(i)"
      done(true)
    }
    let delete = UIContextualAction(style: .destructive, title: "Delete") { _, _, done in done(false) }
    return UISwipeActionsConfiguration(actions: [delete, edit])
  }
}
