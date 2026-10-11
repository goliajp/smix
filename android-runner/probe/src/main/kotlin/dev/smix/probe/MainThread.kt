package dev.smix.probe

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Running a read of the app's UI on the app's main thread, from the binder
 * thread a caller's question arrives on.
 *
 * Compose's layout nodes belong to the main thread. Walked from a binder
 * thread, the semantics tree can be met half way through a layout pass —
 * on the fixture's tour screen, which re-lays out forty rows every frame,
 * 140 of 300 such reads came back without the overlay that was on screen
 * the whole time, and one run of them ended the app with a
 * NullPointerException inside `LayoutNode.remeasure` on the main thread.
 * Read on the main thread, 300 of 300 were whole.
 */
internal object MainThread {
    sealed interface Ran<out T> {
        data class Done<T>(val value: T) : Ran<T>
        data class Threw(val error: Exception) : Ran<Nothing>
        object Busy : Ran<Nothing>
    }

    /**
     * [read] on the main thread once [ready] says so, or [Ran.Busy] when
     * that has not happened within [waitMs]. A read still queued when the
     * wait ends runs later and its result is dropped.
     *
     * Asked first ahead of anything queued; when not [ready], asked again
     * behind what is queued, which is where the pending layout pass is.
     */
    fun <T> run(waitMs: Long, ready: () -> Boolean = { true }, read: () -> T): Ran<T> {
        val deadline = SystemClock.uptimeMillis() + waitMs
        if (Looper.myLooper() == Looper.getMainLooper()) return attempt(read)
        var ran: Ran<T> = Ran.Busy
        val done = CountDownLatch(1)
        val main = Handler(Looper.getMainLooper())
        val step = object : Runnable {
            override fun run() {
                if (!ready() && SystemClock.uptimeMillis() < deadline) {
                    main.post(this)
                    return
                }
                ran = attempt(read)
                done.countDown()
            }
        }
        main.postAtFrontOfQueue(step)
        return if (done.await(waitMs, TimeUnit.MILLISECONDS)) ran else Ran.Busy
    }

    // Thrown on the main thread, this would end the app under test.
    private fun <T> attempt(read: () -> T): Ran<T> = try {
        Ran.Done(read())
    } catch (e: Exception) {
        Ran.Threw(e)
    }
}
