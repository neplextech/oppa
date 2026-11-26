package com.neplextech.oppa.mobile

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.FileObserver
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject
import java.io.File

/**
 * Keeps the OPPA process at foreground priority so the Rust agent can hold its
 * OpenPrinter connection while the user is in another app or the screen is
 * locked.
 *
 * The service owns no agent logic. The agent writes a sanitized status file
 * that this service observes to update its notification; the Stop action is
 * delivered to the agent through [AgentBridge].
 */
class AgentForegroundService : Service() {
    private var statusObserver: FileObserver? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        ensureChannels(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            Logs.info("stop requested from the notification")
            if (AgentBridge.runtimeAttached) {
                AgentBridge.send("stopRequested")
            }
            stopForegroundCompat()
            stopSelf()
            return START_NOT_STICKY
        }

        val status = readStatus(this)
        if (!AgentBridge.runtimeAttached) {
            // The system restarted a sticky service after killing the process.
            // The Rust agent starts with the app, so ask the user to reopen it
            // instead of claiming a connection that does not exist.
            startForegroundCompat(buildNotification(this, status, restarted = true))
            notifyRestartRequired(this)
            stopForegroundCompat()
            stopSelf()
            return START_NOT_STICKY
        }

        startForegroundCompat(buildNotification(this, status, restarted = false))
        acquireLocks()
        observeStatus()
        running = true
        return START_STICKY
    }

    override fun onDestroy() {
        running = false
        statusObserver?.stopWatching()
        statusObserver = null
        releaseLocks()
        super.onDestroy()
    }

    private fun startForegroundCompat(notification: Notification) {
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE
        } else {
            0
        }
        ServiceCompat.startForeground(this, NOTIFICATION_ID, notification, type)
    }

    private fun stopForegroundCompat() {
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    }

    @SuppressLint("WakelockTimeout")
    private fun acquireLocks() {
        if (wakeLock == null) {
            val power = getSystemService(Context.POWER_SERVICE) as PowerManager
            wakeLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "oppa:agent").apply {
                setReferenceCounted(false)
                acquire()
            }
        }
        if (wifiLock == null) {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            @Suppress("DEPRECATION")
            val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                WifiManager.WIFI_MODE_FULL_LOW_LATENCY
            } else {
                WifiManager.WIFI_MODE_FULL_HIGH_PERF
            }
            wifiLock = wifi.createWifiLock(mode, "oppa:agent").apply {
                setReferenceCounted(false)
                acquire()
            }
        }
    }

    private fun releaseLocks() {
        wakeLock?.takeIf { it.isHeld }?.release()
        wakeLock = null
        wifiLock?.takeIf { it.isHeld }?.release()
        wifiLock = null
    }

    private fun observeStatus() {
        if (statusObserver != null) return
        val path = statusPath(this) ?: return
        val file = File(path)
        val directory = file.parentFile ?: return
        val mask = FileObserver.CLOSE_WRITE or FileObserver.MOVED_TO
        val observer = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            object : FileObserver(directory, mask) {
                override fun onEvent(event: Int, name: String?) = onStatusEvent(name, file.name)
            }
        } else {
            @Suppress("DEPRECATION")
            object : FileObserver(directory.path, mask) {
                override fun onEvent(event: Int, name: String?) = onStatusEvent(name, file.name)
            }
        }
        observer.startWatching()
        statusObserver = observer
        // The agent may have rewritten the file between the first read in
        // onStartCommand and startWatching; catch up so the notification
        // never stays on a stale state such as "Connecting".
        onStatusEvent(file.name, file.name)
    }

    private fun onStatusEvent(name: String?, expected: String) {
        if (name != expected) return
        val status = readStatus(this)
        if (status?.optBoolean("paired", true) == false) {
            // Forgetting the server ends background operation.
            stopForegroundCompat()
            stopSelf()
            return
        }
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.notify(NOTIFICATION_ID, buildNotification(this, status, restarted = false))
    }

    companion object {
        const val ACTION_STOP = "com.neplextech.oppa.mobile.STOP_AGENT"
        private const val CHANNEL_ID = "oppa-agent"
        private const val ALERT_CHANNEL_ID = "oppa-agent-alerts"
        private const val NOTIFICATION_ID = 7410
        private const val RESTART_NOTIFICATION_ID = 7411
        const val PREFERENCES = "oppa_background_agent"
        const val KEY_ENABLED = "enabled"
        const val KEY_STATUS_PATH = "statusPath"

        @Volatile
        var running: Boolean = false
            private set

        fun start(context: Context) {
            val intent = Intent(context, AgentForegroundService::class.java)
            ContextCompat.startForegroundService(context, intent)
        }

        /**
         * Re-posts the running service's notification. Android 13+ does not
         * show a foreground notification posted before POST_NOTIFICATIONS was
         * granted until it is posted again.
         */
        fun refreshNotification(context: Context) {
            if (!running) return
            val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            manager.notify(NOTIFICATION_ID, buildNotification(context, readStatus(context), restarted = false))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, AgentForegroundService::class.java))
        }

        fun preferences(context: Context) =
            context.applicationContext.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

        fun statusPath(context: Context): String? =
            preferences(context).getString(KEY_STATUS_PATH, null)

        fun readStatus(context: Context): JSONObject? {
            val path = statusPath(context) ?: return null
            return try {
                val file = File(path)
                if (!file.isFile || file.length() > 4096) null else JSONObject(file.readText())
            } catch (error: Exception) {
                null
            }
        }

        fun ensureChannels(context: Context) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
            val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            val label = context.applicationInfo.loadLabel(context.packageManager).toString()
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "$label agent", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "Shows that the print agent is running and connected."
                    setShowBadge(false)
                }
            )
            manager.createNotificationChannel(
                NotificationChannel(ALERT_CHANNEL_ID, "$label alerts", NotificationManager.IMPORTANCE_DEFAULT).apply {
                    description = "Tells you when the print agent stopped and needs to be reopened."
                }
            )
        }

        private fun launchIntent(context: Context): PendingIntent? {
            val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
                ?: return null
            launch.flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED
            return PendingIntent.getActivity(
                context, 0, launch,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
        }

        fun buildNotification(context: Context, status: JSONObject?, restarted: Boolean): Notification {
            val label = context.applicationInfo.loadLabel(context.packageManager).toString()
            val title = if (restarted) {
                "$label stopped"
            } else {
                status?.optString("title")?.takeIf { it.isNotBlank() } ?: "$label agent is running"
            }
            val text = if (restarted) {
                "Android closed the agent. Tap to reopen it and resume printing."
            } else {
                status?.optString("text")?.takeIf { it.isNotBlank() } ?: "Waiting for print jobs"
            }
            val builder = NotificationCompat.Builder(context, CHANNEL_ID)
                .setSmallIcon(R.drawable.oppa_ic_stat_agent)
                .setContentTitle(title)
                .setContentText(text)
                .setStyle(NotificationCompat.BigTextStyle().bigText(text))
                .setOngoing(!restarted)
                .setOnlyAlertOnce(true)
                .setShowWhen(false)
                .setCategory(NotificationCompat.CATEGORY_SERVICE)
                .setPriority(NotificationCompat.PRIORITY_LOW)
                .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
                .setContentIntent(launchIntent(context))
            if (!restarted) {
                val stop = PendingIntent.getService(
                    context, 1,
                    Intent(context, AgentForegroundService::class.java).setAction(ACTION_STOP),
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
                )
                builder.addAction(0, "Stop agent", stop)
            }
            return builder.build()
        }

        fun notifyRestartRequired(context: Context) {
            val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            val notification = NotificationCompat.Builder(context, ALERT_CHANNEL_ID)
                .setSmallIcon(R.drawable.oppa_ic_stat_agent)
                .setContentTitle(
                    "${context.applicationInfo.loadLabel(context.packageManager)} stopped"
                )
                .setContentText("Android closed the agent. Tap to reopen it and resume printing.")
                .setAutoCancel(true)
                .setContentIntent(launchIntent(context))
                .build()
            manager.notify(RESTART_NOTIFICATION_ID, notification)
        }
    }
}
