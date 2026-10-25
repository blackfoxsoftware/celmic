import AVFoundation
import ExpoModulesCore

struct StartOptions: Record {
  @Field var hosts: [String] = []
  @Field var port: Int = 47800
  @Field var code: String = ""
  @Field var deviceName: String = "iPhone"
  @Field var voiceProcessing: Bool = true
}

public class CelmicAudioModule: Module {
  public func definition() -> ModuleDefinition {
    Name("CelmicAudio")

    Events("onStateChange", "onStats")

    OnCreate {
      StreamEngine.shared.onState = { [weak self] state in
        self?.sendEvent("onStateChange", state)
      }
      StreamEngine.shared.onStats = { [weak self] stats in
        self?.sendEvent("onStats", stats)
      }
    }

    OnDestroy {
      StreamEngine.shared.onState = nil
      StreamEngine.shared.onStats = nil
    }

    AsyncFunction("requestPermissionsAsync") { (promise: Promise) in
      let respond: (Bool) -> Void = { granted in
        promise.resolve(["granted": granted, "status": granted ? "granted" : "denied"])
      }
      if #available(iOS 17.0, *) {
        AVAudioApplication.requestRecordPermission(completionHandler: respond)
      } else {
        AVAudioSession.sharedInstance().requestRecordPermission(respond)
      }
    }

    // Android-only concern; kept for a uniform JS API.
    AsyncFunction("requestNotificationPermissionsAsync") { () -> [String: Any] in
      ["granted": true, "status": "granted"]
    }

    Function("start") { (options: StartOptions) in
      StreamEngine.shared.start(config: StreamConfig(
        hosts: options.hosts,
        port: UInt16(clamping: options.port),
        code: options.code,
        deviceName: options.deviceName,
        voiceProcessing: options.voiceProcessing
      ))
    }

    Function("stop") {
      StreamEngine.shared.stop()
    }

    Function("setMuted") { (muted: Bool) in
      StreamEngine.shared.muted = muted
    }

    Function("getState") { () -> [String: Any] in
      StreamEngine.shared.lastState.compactMapValues { $0 }
    }

    AsyncFunction("discoverAsync") { (timeoutMs: Int, promise: Promise) in
      let discovery = Discovery()
      discovery.discover(timeout: Double(timeoutMs) / 1000) { results in
        _ = discovery // keep alive until done
        promise.resolve(results)
      }
    }
  }
}
