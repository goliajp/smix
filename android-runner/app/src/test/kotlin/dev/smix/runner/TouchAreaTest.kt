package dev.smix.runner

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TouchAreaTest {
    private val parent = intArrayOf(100, 500, 1000, 800)

    @Test
    fun aRegionIsMovedFromTheDelegatingViewToTheScreen() {
        assertArrayEquals(
            intArrayOf(110, 520, 250, 660),
            TouchArea.onScreen(intArrayOf(10, 20, 150, 160), parent),
        )
    }

    @Test
    fun aRegionIsCutToTheDelegatingView() {
        assertArrayEquals(
            intArrayOf(100, 500, 250, 800),
            TouchArea.onScreen(intArrayOf(-30, -40, 150, 400), parent),
        )
        assertNull(TouchArea.onScreen(intArrayOf(950, 0, 1200, 100), parent))
    }
}
