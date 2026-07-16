// Swift conformance fixture runner.
//
// Usage:
//   swift run --package-path swift-bridge SwiftFixtureRunner <path-to-fixture.json>
//
// Loads a conformance fixture JSON (same schema as
// crates/smix-core-conformance/fixtures/*.json), passes tree + selector
// JSON to SmixCoreFFIBindings.resolveSelector, prints the actual id
// list as a sorted JSON array to stdout, exits 0 if matches expected.
//
// Designed for byte-identical diff against the Rust backend
// (`cargo run -p smix-core-conformance --bin fixture-runner -- rust <id>`).
// See scripts/sdk/run-cross-binary-harness.sh.

import Foundation
import SmixCoreFFIBindings

@main
struct SwiftFixtureRunner {
    static func main() {
        let args = CommandLine.arguments
        guard args.count == 2 else {
            FileHandle.standardError.write(
                "usage: SwiftFixtureRunner <path-to-fixture.json>\n".data(using: .utf8)!
            )
            exit(2)
        }
        let path = args[1]

        let data: Data
        do {
            data = try Data(contentsOf: URL(fileURLWithPath: path))
        } catch {
            FileHandle.standardError.write(
                "load failed (\(path)): \(error)\n".data(using: .utf8)!
            )
            exit(2)
        }

        // Fixture wire shape:
        //   {id, description, tree, selector, expected: [String]}
        // We decode tree + selector as raw JSON Values via serialization
        // to preserve byte-for-byte fidelity to Rust resolver inputs.
        let fixture: [String: Any]
        do {
            guard let parsed = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                throw NSError(domain: "fixture", code: 1, userInfo: [NSLocalizedDescriptionKey: "fixture is not a JSON object"])
            }
            fixture = parsed
        } catch {
            FileHandle.standardError.write(
                "parse fixture failed: \(error)\n".data(using: .utf8)!
            )
            exit(2)
        }

        guard let treeJson = encodeAsJsonString(fixture["tree"]),
              let selectorJson = encodeAsJsonString(fixture["selector"]) else {
            FileHandle.standardError.write(
                "could not re-encode tree/selector to JSON strings\n".data(using: .utf8)!
            )
            exit(2)
        }

        let actual: [String]
        do {
            actual = try resolveSelector(treeJson: treeJson, selectorJson: selectorJson)
        } catch {
            FileHandle.standardError.write(
                "FFI raised: \(error)\n".data(using: .utf8)!
            )
            exit(2)
        }

        let sortedActual = actual.sorted()
        // Emit deterministic JSON: sorted ids array.
        let outData = (try? JSONEncoder().encode(sortedActual)) ?? Data()
        if let line = String(data: outData, encoding: .utf8) {
            print(line)
        }

        if let expected = fixture["expected"] as? [String] {
            let sortedExpected = expected.sorted()
            if sortedActual != sortedExpected {
                FileHandle.standardError.write(
                    "MISMATCH:\n  expected: \(sortedExpected)\n  actual:   \(sortedActual)\n".data(using: .utf8)!
                )
                exit(1)
            }
        }
        exit(0)
    }

    /// Re-encode a JSON value (`[String: Any]` etc.) as a JSON string
    /// suitable for the FFI boundary. Uses `.sortedKeys` for stable output.
    private static func encodeAsJsonString(_ value: Any?) -> String? {
        guard let value else { return nil }
        guard let data = try? JSONSerialization.data(
            withJSONObject: value,
            options: [.sortedKeys]
        ) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
