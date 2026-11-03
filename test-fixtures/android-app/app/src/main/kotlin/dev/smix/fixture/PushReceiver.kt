package dev.smix.fixture

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import org.json.JSONException
import org.json.JSONObject

/// What `sendPush` reaches on Android. smix sends the explicit broadcast
/// `dev.smix.PUSH` with `title`, `body` and `data` (the payload's other
/// keys, as a JSON object in text), and the app posts the notification
/// itself — on a channel important enough to show as a heads-up, as a
/// real push would. Tapping it opens [PushActivity] with what it carried.
class PushReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val title = intent.getStringExtra("title").orEmpty()
        val body = intent.getStringExtra("body").orEmpty()
        // the extras come from outside the app: a payload that is not an
        // object carries no route
        val route = try {
            JSONObject(intent.getStringExtra("data") ?: "{}").optString("route", "none")
        } catch (_: JSONException) {
            "none"
        }
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Pushes", NotificationManager.IMPORTANCE_HIGH),
        )
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, PushActivity::class.java)
                .putExtra(PushActivity.OPENED, "opened $title → $route")
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        manager.notify(
            (System.currentTimeMillis() % Int.MAX_VALUE).toInt(),
            Notification.Builder(context, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle(title)
                .setContentText(body)
                .setContentIntent(open)
                .setAutoCancel(true)
                .build(),
        )
    }

    companion object {
        const val CHANNEL = "smix-push"
    }
}
