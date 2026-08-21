// A swift-testing suite with the shape a real one has: a @Suite, @Test
// cases carrying the sentence as their name, and helpers between them.
// The claims sit above the cases they belong to.
//
// Not compiled — this is a fixture for the claim scanner, which reads
// lines and parses no language.

import Testing

@Suite("Menu entries")
struct MenuEntriesTests {
    private func entries(hiding: [Int]) -> [MenuEntry] { [] }

    // covers: CTR-MENU-0001
    @Test("every section is separated when nothing is hidden")
    func everySectionSeparated() {}

    // covers: CTR-MENU-0002
    @Test("hiding part of a section keeps its separator")
    func partialHidingKeepsSeparator() {}

    // covers: CTR-MENU-0003, CTR-MENU-0004
    @Test("an entirely hidden middle section takes its separator with it")
    func hiddenMiddleSection() {}

    // A case that was commented out while its failure is investigated.
    // The claim line above it is still an ordinary comment, and the
    // scanner reads lines.
    // covers: CTR-MENU-0005
    // @Test("no separator trails the list when the last section is hidden")
    // func lastSectionHidden() {}
}
