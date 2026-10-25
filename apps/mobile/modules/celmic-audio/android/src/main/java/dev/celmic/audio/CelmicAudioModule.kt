package dev.celmic.audio

import android.Manifest
import android.content.Context
import android.os.Build
import expo.modules.interfaces.permissions.Permissions
import expo.modules.kotlin.Promise
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.records.Field
import expo.modules.kotlin.records.Record

class StartOptions : Record {
  @Field val hosts: List<String> = emptyList()
  @Field val port: Int = 47800
  @Field val code: String = ""
  @Field val deviceName: String = Build.MODEL
  @Field val voiceProcessing: Boolean = true
}

class CelmicAudioModule : Module() {
  private val context: Context
    get() = appContext.reactContext ?: throw Exceptions.ReactContextLost()

  override fun definition() = ModuleDefinition {
    Name("CelmicAudio")

    Events("onStateChange", "onStats")

    OnCreate {
      StreamEngine.listener = object : StreamListener {
        override fun onState(state: Map<String, Any?>) = sendEvent("onStateChange", state)
        override fun onStats(stats: Map<String, Any?>) = sendEvent("onStats", stats)
      }
    }

    OnDestroy {
      StreamEngine.listener = null
    }

    AsyncFunction("requestPermissionsAsync") { promise: Promise ->
      Permissions.askForPermissionsWithPermissionsManager(
        appContext.permissions,
        promise,
        Manifest.permission.RECORD_AUDIO,
      )
    }

    // Separate from the mic so a denied notification doesn't block streaming.
    AsyncFunction("requestNotificationPermissionsAsync") { promise: Promise ->
      if (Build.VERSION.SDK_INT >= 33) {
        Permissions.askForPermissionsWithPermissionsManager(
          appContext.permissions,
          promise,
          Manifest.permission.POST_NOTIFICATIONS,
        )
      } else {
        promise.resolve(mapOf("granted" to true, "status" to "granted"))
      }
    }

    Function("start") { options: StartOptions ->
      StreamService.start(
        context,
        StreamConfig(
          hosts = options.hosts,
          port = options.port,
          code = options.code,
          deviceName = options.deviceName,
          voiceProcessing = options.voiceProcessing,
        ),
      )
    }

    Function("stop") {
      StreamEngine.stop()
      StreamService.stop(context)
    }

    Function("setMuted") { muted: Boolean ->
      StreamEngine.muted = muted
    }

    Function("getState") {
      StreamEngine.lastState
    }

    AsyncFunction("discoverAsync") { timeoutMs: Int, promise: Promise ->
      Discovery(context).discover(timeoutMs.toLong(), promise)
    }
  }
}
