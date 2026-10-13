package dev.rurarendar.app;

import android.app.AlarmManager;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.os.Build;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

final class NotificationBridge {
    static final String CHANNEL_ID = "event-alerts";
    private static final String PREFS = "rurarendar-notifications";
    private static final String QUEUE = "queue";
    private static final String DELIVERED = "delivered";
    private static final int ALARM_REQUEST = 1;

    private NotificationBridge() {}

    static void createChannel(Context context) {
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationChannel channel = new NotificationChannel(
                    CHANNEL_ID, "Event alerts", NotificationManager.IMPORTANCE_HIGH);
            manager(context).createNotificationChannel(channel);
        }
    }

    public static void scheduleAll(Context context, String serialized) {
        JSONArray incoming;
        JSONArray delivered = readArray(context, DELIVERED);
        JSONArray filtered = new JSONArray();
        long cutoff = System.currentTimeMillis() - 365L * 24L * 60L * 60L * 1000L;
        for (int index = 0; index < delivered.length(); index++) {
            JSONObject item = delivered.optJSONObject(index);
            if (item != null && item.optLong("at", 0) >= cutoff) {
                filtered.put(item);
            }
        }
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit()
                .putString(DELIVERED, filtered.toString())
                .apply();
        try {
            incoming = new JSONArray(serialized);
        } catch (JSONException error) {
            incoming = new JSONArray();
        }
        JSONArray pending = new JSONArray();
        for (int index = 0; index < incoming.length(); index++) {
            JSONObject candidate = incoming.optJSONObject(index);
            if (candidate != null && !wasDelivered(filtered, candidate.optString("id"))) {
                pending.put(candidate);
            }
        }
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit()
                .putString(QUEUE, pending.toString())
                .apply();
        scheduleNext(context);
    }

    static void scheduleNext(Context context) {
        createChannel(context);
        JSONArray queue = readArray(context, QUEUE);

        long now = System.currentTimeMillis();
        while (queue.length() > 0) {
            JSONObject next = queue.optJSONObject(0);
            if (next == null) {
                queue.remove(0);
                continue;
            }
            long at = next.optLong("at", 0);
            if (at < now - 24L * 60L * 60L * 1000L) {
                queue.remove(0);
                continue;
            }
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                    .edit()
                    .putString(QUEUE, queue.toString())
                    .apply();
            Intent intent = new Intent(context, NotificationReceiver.class)
                    .setAction("dev.rurarendar.app.SHOW_EVENT_ALERT")
                    .putExtra("id", next.optString("id"))
                    .putExtra("title", next.optString("title"))
                    .putExtra("body", next.optString("body"));
            PendingIntent pending = PendingIntent.getBroadcast(
                    context, ALARM_REQUEST, intent,
                    PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            AlarmManager alarms = (AlarmManager) context.getSystemService(Context.ALARM_SERVICE);
            if (alarms != null) {
                long trigger = Math.max(at, now + 1000L);
                if (Build.VERSION.SDK_INT >= 31 && !alarms.canScheduleExactAlarms()) {
                    alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, trigger, pending);
                } else {
                    alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, trigger, pending);
                }
            }
            return;
        }
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit()
                .putString(QUEUE, queue.toString())
                .apply();
    }

    static void deliver(Context context, String title, String body, String id) {
        if (!canPostNotifications(context)) {
            return;
        }
        createChannel(context);
        Notification.Builder builder = new Notification.Builder(context, CHANNEL_ID);
        builder.setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle(title)
                .setContentText(body)
                .setStyle(new Notification.BigTextStyle().bigText(body))
                .setAutoCancel(true);
        Intent launch = context.getPackageManager().getLaunchIntentForPackage(context.getPackageName());
        if (launch != null) {
            PendingIntent content = PendingIntent.getActivity(
                    context, id.hashCode(), launch,
                    PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            builder.setContentIntent(content);
        }
        manager(context).notify(id.hashCode(), builder.build());
    }

    static boolean canPostNotifications(Context context) {
        return Build.VERSION.SDK_INT < 33
                || context.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS)
                        == PackageManager.PERMISSION_GRANTED;
    }

    static void removeDelivered(Context context, String id) {
        SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        JSONArray queue = readArray(context, QUEUE);
        JSONArray delivered = readArray(context, DELIVERED);
        JSONArray remaining = new JSONArray();
        for (int index = 0; index < queue.length(); index++) {
            JSONObject item = queue.optJSONObject(index);
            if (item != null && id.equals(item.optString("id"))) {
                JSONObject receipt = new JSONObject();
                try {
                    receipt.put("id", id);
                    receipt.put("at", item.optLong("at", System.currentTimeMillis()));
                    delivered.put(receipt);
                } catch (JSONException ignored) {
                }
            } else if (item != null) {
                remaining.put(item);
            }
        }
        prefs.edit()
                .putString(QUEUE, remaining.toString())
                .putString(DELIVERED, delivered.toString())
                .apply();
    }

    private static JSONArray readArray(Context context, String key) {
        try {
            return new JSONArray(context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                    .getString(key, "[]"));
        } catch (JSONException error) {
            return new JSONArray();
        }
    }

    private static boolean wasDelivered(JSONArray delivered, String id) {
        for (int index = 0; index < delivered.length(); index++) {
            JSONObject item = delivered.optJSONObject(index);
            if (item != null && id.equals(item.optString("id"))) {
                return true;
            }
        }
        return false;
    }

    private static NotificationManager manager(Context context) {
        return (NotificationManager) context.getSystemService(Context.NOTIFICATION_SERVICE);
    }
}