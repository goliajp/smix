// Which app the probe routes ask about.
//
// The probe answers at `content://<applicationId>.smixprobe`, so the
// routes need an applicationId before they can ask anything. A flow
// always sends one on `App-Bundle-Id`. The CLI never did, and the
// consequence was not a refusal but a different answer: `smix tree`
// read the accessibility projection while a flow standing on the same
// screen read the semantics tree, and neither said the other existed.
//
// The runner can answer without being told, because it can see which
// window holds the focus. That inference belongs here and not on the
// host: the host cannot see the window stack, and a second inference
// beside this one would be a second implementation of the same
// decision, free to drift.
//
// **Only when nobody said.** A name that arrived is used verbatim.
//
// This is deliberately NOT the rule `WindowRules.isForeignPopup` uses.
// There, guessing the app under test made a permission dialog classify
// itself as the app it was covering, and the route reported nothing.
// Here a wrong guess costs nothing of that kind: asking a package that
// carries no probe answers `present:false`, which is the same answer as
// asking about nothing, and the caller falls through to the
// accessibility tree it has always had.
package dev.smix.runner

object ProbeTarget {

    /// The applicationId to ask about, or null when nothing can be asked.
    ///
    /// `named` is what the caller sent — the `App-Bundle-Id` header or
    /// `?app=`. Blank counts as unsent: `?app=` with nothing after it
    /// would otherwise address `content://.smixprobe`.
    fun pick(rows: List<WindowRules.Row>, named: String?): String? {
        if (!named.isNullOrBlank()) return named
        return rows
            .asSequence()
            // Only an ordinary app window can be the app under test.
            // The keyboard takes the focus while a field is being
            // filled, and the system bars report as active on some
            // builds; either would be picked by layer order alone.
            .filter { it.type == WindowRules.TYPE_APPLICATION && it.takesFocus }
            // `root.packageName` is null once the window's root has
            // gone, and `content://null.smixprobe` is not a question.
            .filter { !it.pkg.isNullOrBlank() }
            // Two app windows can carry focus flags at once during a
            // transition; the one in front is the one on the screen.
            .maxByOrNull { it.layer }
            ?.pkg
    }
}
