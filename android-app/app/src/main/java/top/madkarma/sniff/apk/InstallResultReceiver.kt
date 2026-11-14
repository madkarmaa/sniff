package top.madkarma.sniff.apk

import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import androidx.core.content.IntentCompat
import top.madkarma.sniff.R
import top.madkarma.sniff.data.uiText

/**
 * Receives the status of an install session; it is not exported, only our PendingIntent reaches it.
 */
class InstallResultReceiver : BroadcastReceiver() {
    // The launched intent is the system's confirmation screen, delivered through our own
    // PendingIntent.
    @SuppressLint("UnsafeIntentLaunch")
    override fun onReceive(context: Context, intent: Intent) {
        val sessionId = intent.getIntExtra(PackageInstaller.EXTRA_SESSION_ID, -1)
        val status =
            intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)

        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            val confirmation =
                IntentCompat.getParcelableExtra(intent, Intent.EXTRA_INTENT, Intent::class.java)
            val launched =
                confirmation != null &&
                    try {
                        context.startActivity(confirmation.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        true
                    } catch (_: Exception) {
                        false
                    }

            if (!launched) {
                val failed =
                    InstallOutcome(
                        sessionId,
                        PackageInstaller.STATUS_FAILURE,
                        uiText(R.string.error_install_failed),
                    )
                InstallEvents.publish(failed)
            }
            return
        }

        val message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
        InstallEvents.publish(InstallOutcome(sessionId, status, installStatusText(status, message)))
    }
}
