import UIKit

/// A list whose rows all carry one identifier, and a search that comes up
/// over it as a form sheet — a card on an iPad, the whole screen on a phone —
/// with its own row under the same identifier and the keyboard up. The shape
/// of a consumer's device search: on an iPad the rows behind the card stayed
/// in the tree, and `tap id:device-row` took the first of them, under the
/// card, and reported it tapped.
final class SheetSameIdsController: UIViewController, UITableViewDataSource, UITableViewDelegate {
  private let names = (0..<40).map { "Lobby Entrance \($0)" }
  private let table = UITableView()
  private let picked = UILabel()

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    picked.text = "picked nothing"
    picked.accessibilityIdentifier = "picked"
    let search = UIButton(type: .system)
    search.setTitle("Search", for: .normal)
    search.accessibilityIdentifier = "open-search"
    search.addAction(UIAction { [weak self] _ in self?.openSearch() }, for: .touchUpInside)
    table.dataSource = self
    table.delegate = self
    table.rowHeight = 53
    table.register(UITableViewCell.self, forCellReuseIdentifier: "row")
    for v in [picked, search, table] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    let guide = view.safeAreaLayoutGuide
    NSLayoutConstraint.activate([
      picked.topAnchor.constraint(equalTo: guide.topAnchor, constant: 8),
      picked.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      search.topAnchor.constraint(equalTo: picked.bottomAnchor, constant: 8),
      search.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      table.topAnchor.constraint(equalTo: search.bottomAnchor, constant: 8),
      table.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      table.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      table.bottomAnchor.constraint(equalTo: view.bottomAnchor),
    ])
  }

  // Some row's top sits 10 points above the screen's bottom edge, whatever
  // the iPad: it shows a sliver and its centre is off the screen — the row a
  // consumer's tap was sent to, with the row on the sheet left unpicked.
  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    let target = view.bounds.height - 10 - table.frame.minY
    let inset = target.truncatingRemainder(dividingBy: table.rowHeight)
    if table.contentInset.top != inset {
      table.contentInset.top = inset
      table.contentOffset.y = -inset
    }
  }

  func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { names.count }

  func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
    let cell = tableView.dequeueReusableCell(withIdentifier: "row", for: indexPath)
    cell.textLabel?.text = names[indexPath.row]
    cell.accessibilityIdentifier = "device-row"
    cell.accessibilityLabel = names[indexPath.row]
    return cell
  }

  func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
    tableView.deselectRow(at: indexPath, animated: false)
    picked.text = "picked \(names[indexPath.row])"
  }

  private func openSearch() {
    let sheet = SearchSheet { [weak self] name in
      self?.picked.text = "picked \(name)"
      self?.dismiss(animated: true)
    }
    sheet.modalPresentationStyle = .formSheet
    present(sheet, animated: true)
  }
}

private final class SearchSheet: UIViewController {
  private let onPick: (String) -> Void
  private let field = UITextField()

  init(onPick: @escaping (String) -> Void) {
    self.onPick = onPick
    super.init(nibName: nil, bundle: nil)
  }

  required init?(coder: NSCoder) { fatalError("not from a nib") }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    field.placeholder = "Search devices"
    field.borderStyle = .roundedRect
    field.accessibilityIdentifier = "input-device-search"
    let found = UILabel()
    found.text = "1 result found"
    found.accessibilityIdentifier = "search-count"
    let row = UIButton(type: .system)
    row.setTitle("Lobby Elevator", for: .normal)
    row.accessibilityIdentifier = "device-row"
    row.accessibilityLabel = "Lobby Elevator, Main Lobby"
    row.addAction(UIAction { [weak self] _ in self?.onPick("Lobby Elevator") }, for: .touchUpInside)
    for v in [field, found, row] as [UIView] {
      v.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(v)
    }
    let guide = view.safeAreaLayoutGuide
    NSLayoutConstraint.activate([
      field.topAnchor.constraint(equalTo: guide.topAnchor, constant: 16),
      field.leadingAnchor.constraint(equalTo: guide.leadingAnchor, constant: 16),
      field.trailingAnchor.constraint(equalTo: guide.trailingAnchor, constant: -16),
      found.topAnchor.constraint(equalTo: field.bottomAnchor, constant: 12),
      found.leadingAnchor.constraint(equalTo: field.leadingAnchor),
      row.topAnchor.constraint(equalTo: found.bottomAnchor, constant: 12),
      row.leadingAnchor.constraint(equalTo: field.leadingAnchor),
    ])
  }

  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    field.becomeFirstResponder()
  }
}
