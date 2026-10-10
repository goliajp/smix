package dev.smix.fixture

import android.app.Activity
import android.graphics.Color
import android.graphics.PixelFormat
import android.os.Bundle
import android.view.Gravity
import android.view.WindowManager
import android.widget.TextView

/// A window over everything, holding the focus, with nothing readable of
/// any app behind it — the screen a consumer's `runner up` met when a
/// password manager's prompt or a keyboard's stylus introduction was in
/// front of their app.
///
/// An opaque full-screen overlay covers this activity's own window
/// completely, and a window with nothing left showing is not in the
/// accessibility service's list. Needs `SYSTEM_ALERT_WINDOW`, which the
/// gate grants through `appops`; force-stopping the app takes it away.
class OverlayActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(TextView(this).apply { text = "under the overlay" })
        val cover = TextView(this).apply {
            text = "an overlay in front of everything"
            setBackgroundColor(Color.DKGRAY)
            setTextColor(Color.WHITE)
            gravity = Gravity.CENTER
        }
        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            0,
            PixelFormat.OPAQUE,
        )
        getSystemService(WindowManager::class.java).addView(cover, params)
    }
}
