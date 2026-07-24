// RecordBuffer lifecycle + drain, JVM-unit-tested without a device.

package dev.smix.runner

import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RecordBufferTest {
    private fun click(id: String, t: Long = 1) =
        CapturedAxEvent(RecordMapper.TYPE_VIEW_CLICKED, "com.x:id/$id", null, null, t)

    @After
    fun reset() {
        RecordBuffer.stop()
    }

    @Test
    fun inactiveByDefaultDropsEvents() {
        RecordBuffer.stop() // ensure inactive
        RecordBuffer.append(click("ignored"))
        assertFalse(RecordBuffer.isActive())
        assertTrue(RecordBuffer.poll().isEmpty())
    }

    @Test
    fun startActivatesAndPollDrains() {
        RecordBuffer.start()
        assertTrue(RecordBuffer.isActive())
        RecordBuffer.append(click("a"))
        RecordBuffer.append(click("b"))
        val first = RecordBuffer.poll()
        assertEquals(2, first.size)
        assertEquals("a", JSONObject(first[0]).getJSONObject("selector").getString("id"))
        // Drained: a second poll with no new events is empty.
        assertTrue(RecordBuffer.poll().isEmpty())
    }

    @Test
    fun stopReturnsRemainderAndDeactivates() {
        RecordBuffer.start()
        RecordBuffer.append(click("c"))
        val rest = RecordBuffer.stop()
        assertEquals(1, rest.size)
        assertFalse(RecordBuffer.isActive())
        RecordBuffer.append(click("after")) // dropped
        assertTrue(RecordBuffer.poll().isEmpty())
    }

    @Test
    fun startClearsPriorEvents() {
        RecordBuffer.start()
        RecordBuffer.append(click("old"))
        RecordBuffer.start() // restart clears
        assertTrue(RecordBuffer.poll().isEmpty())
    }
}
