package dev.smix.fixture

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import org.json.JSONObject

/// What an FCM message reaches, without the Firebase SDK: the broadcast
/// Play services sends, with every extra as it arrived. It is posted as a
/// notification whose words are the message's, and whose tap opens
/// [PushActivity] with the extras in one line.
class FcmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val extras = intent.extras ?: return
        val said = JSONObject()
        for (key in extras.keySet().sorted()) {
            // smix's own envelope differs from run to run
            if (key != "google.message_id") said.put(key, extras.getString(key))
        }
        PushReceiver.post(
            context,
            extras.getString("gcm.notification.title") ?: "data",
            extras.getString("gcm.notification.body") ?: said.optString("body"),
            said.toString(),
        )
    }
}
