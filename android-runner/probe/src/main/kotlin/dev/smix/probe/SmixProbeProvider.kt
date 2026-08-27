package dev.smix.probe

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.net.Uri
import android.os.Bundle

/**
 * Arms the probe, and is how another process asks it anything.
 *
 * A content provider rather than a socket: no port to allocate, no
 * INTERNET permission to add to somebody else's app, and no second thing
 * to tear down. It is also the earliest hook there is — providers are
 * created before `Application.onCreate`, which is what puts [install]
 * ahead of the first `setContent`. A callback armed after that would miss
 * every root already made and answer with an empty tree, which reads
 * exactly like an app that has no Compose in it.
 *
 * The authority is derived from the host's package so two apps on one
 * device do not collide.
 */
class SmixProbeProvider : ContentProvider() {
    override fun onCreate(): Boolean {
        SemanticsProbe.install()
        return true
    }

    /**
     * Who is allowed to ask.
     *
     * The provider is exported, because the runner is a different process
     * and there is no other way for it to reach in. Exported means every
     * app on the device can call it — and what it answers with includes
     * `inputText`, which on a password field is the characters themselves.
     * A debug build is not a reason to hand those to whatever else is
     * installed.
     *
     * `callingPackage` comes from the system, not from the caller, so it
     * cannot be spoofed by an app claiming a name. Default-deny, and the
     * refusal names itself: "nothing came back" and "you are not allowed
     * to ask" must not look the same from the outside.
     */
    private fun callerAllowed(): Boolean =
        isAllowedCaller(callingPackage, context?.packageName)

    override fun call(method: String, arg: String?, extras: Bundle?): Bundle =
        Bundle().apply {
            if (!callerAllowed()) {
                putString(
                    KEY_ERROR,
                    "smix probe refuses ${callingPackage ?: "an unnamed caller"}: " +
                        "it answers the host app, its instrumentation, and adb, " +
                        "and this is none of them",
                )
                return@apply
            }
            when (method) {
                // Deliberately not one "status" call returning everything:
                // a caller asking whether the probe is live must not have to
                // pay for a tree dump to find out.
                METHOD_TREE -> {
                    putString(KEY_TREE, SemanticsProbe.dumpWireJson())
                    val (w, h) = SemanticsProbe.screenSize()
                    putInt("screenW", w)
                    putInt("screenH", h)
                }
                METHOD_IDLE -> {
                    putLong(KEY_QUIET_MS, SemanticsProbe.quiescentForMs())
                    putBoolean("pendingLayout", SemanticsProbe.hasPendingLayout())
                }
                // Experimental, and named so. Whether it survives depends on
                // what it is measured to do to a node nothing can touch.
                METHOD_ACT -> putString(
                    KEY_RESULT,
                    SemanticsProbe.act(arg ?: "", extras?.getString(KEY_ACTION) ?: "OnClick"),
                )
                // Not on the offered surface. It is here so a gate can show
                // that the refusal above refuses something real — a rule
                // whose subject has never been observed is a rule nobody
                // has checked.
                METHOD_ACT_UNSAFE -> putString(
                    KEY_RESULT,
                    SemanticsProbe.unsafeAct(arg ?: "", extras?.getString(KEY_ACTION) ?: "OnClick"),
                )
                METHOD_HELLO -> {
                    putString(KEY_VERSION, PROBE_WIRE_VERSION)
                    putInt(KEY_ROOTS, SemanticsProbe.rootCount())
                }
                // Naming the method back is what lets a caller tell "this
                // probe is older than you" from "this probe is broken".
                else -> putString(KEY_ERROR, "unknown method: $method")
            }
        }

    override fun query(
        uri: Uri, projection: Array<out String>?, selection: String?,
        selectionArgs: Array<out String>?, sortOrder: String?,
    ): Cursor? = null

    override fun getType(uri: Uri): String? = null
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, s: String?, a: Array<out String>?): Int = 0
    override fun update(uri: Uri, v: ContentValues?, s: String?, a: Array<out String>?): Int = 0

    companion object {
        /** Bumped when the shape of what `call` returns changes. */
        const val PROBE_WIRE_VERSION = "1"

        const val AUTHORITY_SUFFIX = ".smixprobe"

        /** adb's own package — how a person at a terminal asks. */
        const val SHELL_PACKAGE = "com.android.shell"

        /** smix's instrumentation, which drives from its own process. */
        const val RUNNER_PACKAGE = "dev.smix.runner.test"

        /**
         * The decision, with no Android in it.
         *
         * Out here because the deny path is the half that cannot be proved
         * on a device without installing a second app to be refused — and
         * a rule whose refusal has never been observed is a rule nobody
         * has checked.
         */
        @JvmStatic
        fun isAllowedCaller(caller: String?, host: String?): Boolean = when {
            caller == null -> false
            caller == SHELL_PACKAGE -> true
            caller == RUNNER_PACKAGE -> true
            host != null && caller == host -> true
            else -> false
        }
        const val METHOD_HELLO = "hello"
        const val METHOD_TREE = "tree"
        const val METHOD_IDLE = "idle"
        const val METHOD_ACT = "act"
        const val METHOD_ACT_UNSAFE = "act-unsafe-for-gates"
        const val KEY_ACTION = "action"
        const val KEY_RESULT = "result"
        const val KEY_VERSION = "version"
        const val KEY_ROOTS = "roots"
        const val KEY_TREE = "tree"
        const val KEY_QUIET_MS = "quietMs"
        const val KEY_ERROR = "error"
    }
}
