package dev.rurarendar.app;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

public class NotificationReceiver extends BroadcastReceiver {
    @Override
    public void onReceive(Context context, Intent intent) {
        String action = intent.getAction();
        if (Intent.ACTION_BOOT_COMPLETED.equals(action) || Intent.ACTION_MY_PACKAGE_REPLACED.equals(action)) {
            NotificationBridge.scheduleNext(context);
            return;
        }

        String id = intent.getStringExtra("id");
        if (id != null) {
            if (!NotificationBridge.canPostNotifications(context)) {
                return;
            }
            NotificationBridge.deliver(
                    context,
                    intent.getStringExtra("title"),
                    intent.getStringExtra("body"),
                    id);
            NotificationBridge.removeDelivered(context, id);
        }
        NotificationBridge.scheduleNext(context);
    }
}