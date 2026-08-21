// The JUnit counterpart, with the same requirement wording as the Swift
// suite — which is the finding this whole layer came from: both sides
// already write the same sentence, and nothing could join them.
//
// Not compiled; a fixture for the line scanner.

package app.menu

import org.junit.Test

class MenuEntriesTest {
    private fun destinations(entries: List<MenuEntry>) = entries.map { it.id }

    // covers: CTR-MENU-0001
    @Test
    fun `every section is separated when nothing is hidden`() {}

    // covers: CTR-MENU-0002
    @Test
    fun `hiding part of a section keeps its separator`() {}

    //Covers:CTR-MENU-0003
    @Test
    fun `an entirely hidden middle section takes its separator with it`() {}
}
