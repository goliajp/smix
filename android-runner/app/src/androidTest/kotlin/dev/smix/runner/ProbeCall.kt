package dev.smix.runner

import android.content.Context
import android.net.Uri
import android.os.Bundle

/// One call into the app's probe, over an unstable provider connection that
/// is released before returning.
///
/// `ContentResolver.call` holds a stable reference while the call runs, and
/// the system kills every holder of a stable reference when the provider's
/// process dies. A flow that stops the app while the runner reads its tree
/// took the runner down with it ("depends on provider … in dying proc").
/// An unstable client sees the death as a DeadObjectException instead.
///
/// Throws IllegalArgumentException when nothing declares the authority,
/// the same as `ContentResolver.call`: callers read that as "no probe in
/// this build", which is the ordinary case.
object ProbeCall {
    fun call(ctx: Context, app: String, method: String): Bundle? {
        val uri = Uri.parse("content://$app.smixprobe")
        val client = ctx.contentResolver.acquireUnstableContentProviderClient(uri)
            ?: throw IllegalArgumentException("Unknown authority $app.smixprobe")
        return client.use { it.call(method, null, null) }
    }
}
