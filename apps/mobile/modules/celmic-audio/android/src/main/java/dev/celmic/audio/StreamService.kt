package dev.celmic.audio

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.os.Build
import android.os.IBinder

/**
 * Foreground service (type "microphone") that keeps capture alive with the
 * screen off or the app in the background.
 */
class StreamService : Service() {
  companion object {
    private const val ACTION_STOP = "dev.celmic.audio.STOP"
    private const val CHANNEL_ID = "celmic_stream"
    private const val NOTIFICATION_ID = 47800

    @Volatile private var pendingConfig: StreamConfig? = null

    internal fun start(context: Context, config: StreamConfig) {
      pendingConfig = config
      val intent = Intent(context, StreamService::class.java)
      if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(intent) else context.startService(intent)
    }

    internal fun stop(context: Context) {
      context.stopService(Intent(context, StreamService::class.java))
    }
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      StreamEngine.stop()
      stopSelf()
      return START_NOT_STICKY
    }

    val config = pendingConfig
    pendingConfig = null
    if (config == null) {
      stopSelf()
      return START_NOT_STICKY
    }

    try {
      if (Build.VERSION.SDK_INT >= 29) {
        startForeground(NOTIFICATION_ID, buildNotification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
      } else {
        startForeground(NOTIFICATION_ID, buildNotification())
      }
    } catch (e: Exception) {
      // e.g. microphone permission revoked, or started while the app was in the background.
      StreamEngine.emitState(mapOf("state" to "error", "reason" to "mic", "error" to (e.message ?: e.toString())))
      stopSelf()
      return START_NOT_STICKY
    }

    StreamEngine.onEnded = { stopSelf() }
    StreamEngine.start(this, config)
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    StreamEngine.onEnded = null
    StreamEngine.stop()
    super.onDestroy()
  }

  private fun buildNotification(): Notification {
    val builder = if (Build.VERSION.SDK_INT >= 26) {
      val manager = getSystemService(NotificationManager::class.java)
      manager.createNotificationChannel(
        NotificationChannel(CHANNEL_ID, "Microphone streaming", NotificationManager.IMPORTANCE_LOW),
      )
      Notification.Builder(this, CHANNEL_ID)
    } else {
      @Suppress("DEPRECATION")
      Notification.Builder(this)
    }

    val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    val open = packageManager.getLaunchIntentForPackage(packageName)?.let {
      PendingIntent.getActivity(this, 0, it, flags)
    }
    val stop = PendingIntent.getService(
      this,
      1,
      Intent(this, StreamService::class.java).setAction(ACTION_STOP),
      flags,
    )

    return builder
      .setSmallIcon(android.R.drawable.ic_btn_speak_now)
      .setContentTitle("Celmic")
      .setContentText("Your microphone is streaming to your computer")
      .setOngoing(true)
      .setContentIntent(open)
      .addAction(
        Notification.Action.Builder(
          Icon.createWithResource(this, android.R.drawable.ic_media_pause),
          "Stop",
          stop,
        ).build(),
      )
      .build()
  }
}
