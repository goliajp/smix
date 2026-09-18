// Read Device Hub's front window through the accessibility API: its title,
// and every sidebar row that names a device, with whether it is selected.
//
// Sidebar rows carry `AXIdentifier = TableRow.Device.<UDID>` on their
// cells' children; that is the only place the UDID appears, and it is
// there whether the device is booted or not. `osascript` can read the
// same tree but takes minutes to walk it element by element; this takes
// milliseconds, which is what lets the instrument read before and after
// a boot without the window changing under it.
//
// Output, one per line:
//   title=<window title>
//   row <UDID> selected=<true|false>
// Exit 2 when Device Hub is not running, has no window, or the tree
// could not be read — none of which is "zero rows".
//
// Build and run: swiftc -O -o <out> device-hub-read.swift && <out>
import AppKit
import ApplicationServices

func attr(_ el: AXUIElement, _ name: String) -> AnyObject? {
    var value: AnyObject?
    return AXUIElementCopyAttributeValue(el, name as CFString, &value) == .success ? value : nil
}

func children(_ el: AXUIElement) -> [AXUIElement] {
    (attr(el, kAXChildrenAttribute) as? [AXUIElement]) ?? []
}

func deviceIdentifier(under el: AXUIElement, depth: Int = 0) -> String? {
    if depth > 4 { return nil }
    if let id = attr(el, kAXIdentifierAttribute) as? String, id.hasPrefix("TableRow.Device.") {
        return String(id.dropFirst("TableRow.Device.".count))
    }
    for c in children(el) {
        if let found = deviceIdentifier(under: c, depth: depth + 1) { return found }
    }
    return nil
}

func walk(_ el: AXUIElement, depth: Int, into rows: inout [(String, Bool)]) {
    if depth > 16 { return }
    if (attr(el, kAXRoleAttribute) as? String) == "AXRow" {
        if let udid = deviceIdentifier(under: el) {
            let selected = (attr(el, kAXSelectedAttribute) as? Bool) ?? false
            rows.append((udid, selected))
        }
        return
    }
    for c in children(el) { walk(c, depth: depth + 1, into: &rows) }
}

guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.dt.Devices").first else {
    FileHandle.standardError.write("device-hub-read: Device Hub is not running\n".data(using: .utf8)!)
    exit(2)
}
let axApp = AXUIElementCreateApplication(app.processIdentifier)
guard let windows = attr(axApp, kAXWindowsAttribute) as? [AXUIElement], let window = windows.first else {
    FileHandle.standardError.write("device-hub-read: Device Hub has no window, or its windows could not be read\n".data(using: .utf8)!)
    exit(2)
}
let title = (attr(window, kAXTitleAttribute) as? String) ?? ""
var rows: [(String, Bool)] = []
walk(window, depth: 0, into: &rows)
if rows.isEmpty {
    FileHandle.standardError.write("device-hub-read: read the window but found no device rows — the tree could not be read, which is not an empty sidebar\n".data(using: .utf8)!)
    exit(2)
}
print("title=\(title)")
for (udid, selected) in rows { print("row \(udid) selected=\(selected)") }
