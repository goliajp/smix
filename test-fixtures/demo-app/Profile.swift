import UIKit

/// `-profile`: a settings-style profile screen in the shape a consumer's
/// was — a grouped table whose rows carry a title and a detail in one
/// cell, one of them on a light gray fill, and a red button with white
/// text below it. `profile_result` says which one a touch reached.
///
/// The consumer could not reach the title of such a row by `text`: their
/// rows read title and value as one element ("Time Zone, JST"), which
/// the `Time Zone` row here does too.
final class ProfileController: UIViewController, UITableViewDataSource, UITableViewDelegate {
  private let result = UILabel()
  private let table = UITableView(frame: .zero, style: .insetGrouped)
  private let rows: [(String, String)] = [
    ("Name", "Fixture"), ("Email", "fixture@example.invalid"), ("Plan", "Free"),
    ("Devices", "3"), ("Notifications", "On"), ("Language", "English"),
    ("Region", "Japan"), ("Time Zone", "JST"), ("Storage", "1.2 GB"), ("Sign in on Apple Watch", "Off"),
    ("Privacy", ""), ("Help", ""), ("About", "24"),
  ]

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemGroupedBackground
    result.text = "nothing yet"
    result.accessibilityIdentifier = "profile_result"
    table.dataSource = self
    table.delegate = self
    table.register(UITableViewCell.self, forCellReuseIdentifier: "plain")
    let logout = UIButton(type: .system)
    logout.setTitle("Log out and clear all cache", for: .normal)
    logout.setTitleColor(.white, for: .normal)
    logout.backgroundColor = .systemRed
    logout.layer.cornerRadius = 12
    logout.accessibilityIdentifier = "profile_logout"
    logout.addAction(UIAction { [weak self] _ in self?.result.text = "logged out" }, for: .touchUpInside)
    for v in [result, table, logout] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    NSLayoutConstraint.activate([
      result.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
      result.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
      result.heightAnchor.constraint(equalToConstant: 32),
      table.topAnchor.constraint(equalTo: result.bottomAnchor),
      table.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      table.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      table.bottomAnchor.constraint(equalTo: logout.topAnchor, constant: -12),
      logout.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
      logout.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
      logout.heightAnchor.constraint(equalToConstant: 50),
      logout.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -12),
    ])
  }

  func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
    rows.count
  }

  func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
    let cell = UITableViewCell(style: .value1, reuseIdentifier: nil)
    let (title, detail) = rows[indexPath.row]
    cell.accessoryType = .disclosureIndicator
    // the shape the consumer's rows had: a view of their own that is one
    // accessibility element, its label the title and the value together,
    // with nothing inside it exposed
    if title == "Time Zone" {
      let row = TitleValueRow(title: title, value: detail)
      row.frame = cell.contentView.bounds.insetBy(dx: 20, dy: 0)
      row.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      cell.contentView.addSubview(row)
      return cell
    }
    cell.textLabel?.text = title
    cell.detailTextLabel?.text = detail
    if title == "Sign in on Apple Watch" {
      cell.backgroundColor = .systemGray5
      cell.textLabel?.textColor = .black
    }
    return cell
  }

  func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
    tableView.deselectRow(at: indexPath, animated: false)
    result.text = "opened \(rows[indexPath.row].0)"
  }
}

/// A row drawn by hand that speaks as one element: "Time Zone, JST".
private final class TitleValueRow: UIView {
  init(title: String, value: String) {
    super.init(frame: .zero)
    let left = UILabel()
    left.text = title
    let right = UILabel()
    right.text = value
    right.textColor = .secondaryLabel
    let stack = UIStackView(arrangedSubviews: [left, UIView(), right])
    stack.frame = bounds
    stack.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    addSubview(stack)
    isAccessibilityElement = true
    accessibilityLabel = "\(title), \(value)"
    accessibilityTraits = .button
  }

  required init?(coder: NSCoder) { fatalError("not from a storyboard") }
}
