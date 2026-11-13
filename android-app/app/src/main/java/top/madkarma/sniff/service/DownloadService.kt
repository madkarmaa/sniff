package top.madkarma.sniff.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.PendingIntentCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import top.madkarma.sniff.MainActivity
import top.madkarma.sniff.R

private const val CHANNEL_ID = "downloads"
private const val NOTIFICATION_ID = 1

/**
 * Starts [DownloadService]. The system refuses this while the app is in the background, and the
 * download then runs without the service.
 */
internal fun startDownloadService(context: Context) {
    try {
        ContextCompat.startForegroundService(context, Intent(context, DownloadService::class.java))
    } catch (_: IllegalStateException) {
        // ForegroundServiceStartNotAllowedException (API 31) is an IllegalStateException.
    }
}

/**
 * Holds the process in the foreground state, with an ongoing notification, while a download runs.
 * The download itself stays in the ViewModel; this service only keeps the system from killing the
 * process once the app is in the background. It stops itself as soon as [ActiveWork] is false.
 */
class DownloadService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var watcher: Job? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Must happen promptly after startForegroundService, even if the service stops right after.
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, buildNotification(), foregroundType())
        } catch (_: IllegalStateException) {
            stopSelf()
            return START_NOT_STICKY
        }

        if (watcher?.isActive != true) {
            // A service started while nothing runs, for example after the process was recreated,
            // finds the flag false and stops at once.
            watcher = scope.launch {
                ActiveWork.active.first { !it }
                finish()
            }
        }
        return START_NOT_STICKY
    }

    /** Android 15 ends a dataSync service after its daily time limit; it has to stop at once. */
    override fun onTimeout(startId: Int, fgsType: Int) {
        finish()
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private fun finish() {
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    /** The type is part of startForeground from Android 10 (API 29). */
    private fun foregroundType(): Int =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
        } else {
            0
        }

    private fun buildNotification(): Notification {
        // Creating an existing channel changes nothing, so this is safe to repeat.
        val manager = getSystemService(NOTIFICATION_SERVICE) as NotificationManager
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                getString(R.string.notification_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            )
        )

        val open = PendingIntentCompat.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_UPDATE_CURRENT, false
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(getString(R.string.notification_working))
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .build()
    }
}
