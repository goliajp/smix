// When a wait that polls has run out.
//
// A wait that ends on "not yet" makes a claim about its whole budget, and
// the claim rests on its latest look. Checking the clock after a look lets
// that look be one that began before the budget was over: on a loaded
// emulator one read of every window has taken two seconds, so a look
// begun early reads the old screen, finishes after the deadline, and the
// wait gives up on a state that had already changed. The budget is spent
// only once a look that began after it still says "not yet".

package dev.smix.runner

object Poll {
    /**
     * Look until [done] says yes, or until a look begun once [budgetMs]
     * had passed still says no; returns that last look either way.
     *
     * [look] receives the milliseconds left when it began (never below
     * zero), for looks that carry a wait of their own.
     */
    fun <T> until(
        budgetMs: Long,
        pauseMs: Long,
        look: (leftMs: Long) -> T,
        done: (T) -> Boolean,
        nowMs: () -> Long = { android.os.SystemClock.elapsedRealtime() },
        pause: (Long) -> Unit = { Thread.sleep(it) },
    ): T {
        val deadline = nowMs() + budgetMs
        while (true) {
            val began = nowMs()
            val seen = look(maxOf(deadline - began, 0L))
            if (done(seen) || began >= deadline) return seen
            pause(pauseMs)
        }
    }
}
