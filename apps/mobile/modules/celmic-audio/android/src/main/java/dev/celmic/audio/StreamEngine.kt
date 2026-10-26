package dev.celmic.audio

import android.annotation.SuppressLint
import android.content.Context
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.AudioEffect
import android.media.audiofx.AutomaticGainControl
import android.media.audiofx.NoiseSuppressor
import android.net.wifi.WifiManager
import android.os.Build
import android.os.PowerManager
import android.os.Process
import android.os.SystemClock
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.SocketException
import java.net.SocketTimeoutException
import java.nio.ByteBuffer
import kotlin.concurrent.thread
import kotlin.math.abs

internal data class StreamConfig(
  val hosts: List<String>,
  val port: Int,
  val code: String,
  val deviceName: String,
  val voiceProcessing: Boolean,
)

internal interface StreamListener {
  fun onState(state: Map<String, Any?>)
  fun onStats(stats: Map<String, Any?>)
}

/** Process-wide owner of the (single) active stream. */
internal object StreamEngine {
  @Volatile var listener: StreamListener? = null
  @Volatile var muted = false
  @Volatile var lastState: Map<String, Any?> = mapOf("state" to "idle")
    private set
  /** Invoked when a session ends by itself (error), so the foreground service can stop. */
  @Volatile var onEnded: (() -> Unit)? = null

  private var session: StreamSession? = null

  @Synchronized
  fun start(context: Context, config: StreamConfig) {
    session?.stopAndJoin()
    muted = false
    session = StreamSession(context.applicationContext, config).also { it.start() }
  }

  @Synchronized
  fun stop() {
    val current = session ?: return
    session = null
    current.stopAndJoin()
    emitState(mapOf("state" to "idle"))
  }

  internal fun emitState(state: Map<String, Any?>) {
    lastState = state
    listener?.onState(state)
  }

  internal fun emitStats(stats: Map<String, Any?>) {
    listener?.onStats(stats)
  }

  internal fun ended(ended: StreamSession) {
    synchronized(this) {
      if (session === ended) session = null
    }
    onEnded?.invoke()
  }
}

/**
 * One connection to one desktop. Two threads:
 * - sender: reads the mic in 10 ms frames and sends AUDIO (plus HELLO/PING),
 * - receiver: handles WELCOME / REJECT / PONG.
 */
internal class StreamSession(private val context: Context, private val config: StreamConfig) {
  @Volatile private var running = false
  @Volatile private var session = 0
  @Volatile private var target: InetAddress? = null
  @Volatile private var needHello = true
  @Volatile private var everConnected = false
  @Volatile private var lastPongAt = 0L
  @Volatile private var rttMs = -1.0
  @Volatile private var desktopName: String? = null

  private val socket = DatagramSocket().apply { soTimeout = 1000 }
  private var sender: Thread? = null
  private var receiver: Thread? = null
  private val effects = mutableListOf<AudioEffect>()
  private var wifiLock: WifiManager.WifiLock? = null
  private var wakeLock: PowerManager.WakeLock? = null

  fun start() {
    running = true
    acquireLocks()
    receiver = thread(name = "celmic-recv") { receiveLoop() }
    sender = thread(name = "celmic-send") { sendLoop() }
  }

  fun stopAndJoin() {
    running = false
    sender?.join(1000)
  }

  private fun emit(state: String, extra: Map<String, Any?> = emptyMap()) {
    if (!running) return
    StreamEngine.emitState(mapOf("state" to state, "desktopName" to desktopName) + extra)
  }

  private fun fail(reason: String, message: String) {
    if (!running) return
    StreamEngine.emitState(
      mapOf("state" to "error", "reason" to reason, "error" to message, "desktopName" to desktopName),
    )
    running = false
    StreamEngine.ended(this)
  }

  private fun sendLoop() {
    Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
    var record: AudioRecord? = null
    try {
      val addresses = config.hosts.mapNotNull { runCatching { InetAddress.getByName(it) }.getOrNull() }
      if (addresses.isEmpty()) return fail("network", "No valid computer address.")

      record = openRecorder() ?: return fail("mic", "Could not open the microphone.")
      record.startRecording()
      if (record.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
        return fail("mic", "The microphone is being used by another app.")
      }

      val rate = record.sampleRate
      val frame = rate / 100 // 10 ms
      val pcm = ShortArray(frame)
      val buf = ByteArray(Packets.HEADER_LEN + frame * 2)
      val datagram = DatagramPacket(buf, buf.size)
      val hello = Packets.hello(config.code, config.deviceName)
      val startedAt = SystemClock.elapsedRealtime()
      var seq = 0
      var sent = 0L
      var peak = 0
      var lastHelloAt = 0L
      var lastPingAt = 0L
      var lastStatsAt = 0L

      emit("connecting")
      while (running) {
        val n = record.read(pcm, 0, frame)
        if (n < 0) return fail("mic", "Microphone read failed ($n).")
        if (n == 0) continue
        val now = SystemClock.elapsedRealtime()

        for (i in 0 until n) peak = maxOf(peak, abs(pcm[i].toInt()))
        if (StreamEngine.muted) pcm.fill(0, 0, n)

        // No PONG for a while: the desktop restarted or Wi-Fi dropped. Keep streaming
        // (the session may still be valid) but also start re-announcing ourselves.
        if (session != 0 && !needHello && now - lastPongAt > 4000) {
          needHello = true
          emit("reconnecting")
        }

        if (needHello) {
          if (!everConnected && now - startedAt > 8000) {
            return fail("timeout", "No answer from the computer. Is Celmic open on the same network?")
          }
          if (now - lastHelloAt >= 300) {
            lastHelloAt = now
            for (address in (listOfNotNull(target) + addresses).distinct()) {
              send(DatagramPacket(hello, hello.size, address, config.port))
            }
          }
        }

        val to = target
        val id = session
        if (id != 0 && to != null) {
          Packets.writeHeader(buf, Packets.AUDIO, id, seq++, rate)
          Packets.writePcm(buf, pcm, n)
          datagram.setData(buf, 0, Packets.HEADER_LEN + n * 2)
          datagram.address = to
          datagram.port = config.port
          send(datagram)
          sent++

          if (now - lastPingAt >= 1000) {
            lastPingAt = now
            val payload = ByteBuffer.allocate(8).putLong(SystemClock.elapsedRealtimeNanos()).array()
            val ping = Packets.build(Packets.PING, id, payload)
            send(DatagramPacket(ping, ping.size, to, config.port))
          }
        }

        if (now - lastStatsAt >= 66) {
          lastStatsAt = now
          StreamEngine.emitStats(
            mapOf(
              "level" to peak / 32768.0,
              "rttMs" to rttMs,
              "packetsSent" to sent.toDouble(),
              "sampleRate" to rate,
            ),
          )
          peak = 0
        }
      }
    } catch (e: Exception) {
      fail("network", e.message ?: e.toString())
    } finally {
      record?.let {
        runCatching { it.stop() }
        it.release()
      }
      effects.forEach { it.release() }
      effects.clear()
      val to = target
      if (session != 0 && to != null) {
        val bye = Packets.build(Packets.BYE, session)
        send(DatagramPacket(bye, bye.size, to, config.port))
      }
      socket.close() // also unblocks the receiver
      releaseLocks()
    }
  }

  private fun receiveLoop() {
    val buf = ByteArray(1500)
    val packet = DatagramPacket(buf, buf.size)
    while (running) {
      try {
        socket.receive(packet)
      } catch (_: SocketTimeoutException) {
        continue
      } catch (_: SocketException) {
        break // closed
      }
      val parsed = Packets.parse(buf, packet.length) ?: continue
      when (parsed.type) {
        Packets.WELCOME -> {
          if (parsed.session == 0) continue
          val fresh = needHello || session != parsed.session
          desktopName = String(parsed.payload, Charsets.UTF_8)
          target = packet.address
          session = parsed.session
          lastPongAt = SystemClock.elapsedRealtime()
          needHello = false
          everConnected = true
          if (fresh) emit("streaming")
        }
        Packets.REJECT -> {
          when (parsed.payload.firstOrNull()?.toInt()) {
            Packets.REJECT_BAD_CODE -> fail("bad_code", "Wrong pairing code.")
            Packets.REJECT_UNSUPPORTED_VERSION ->
              fail("unsupported_version", "This computer runs an incompatible Celmic version.")
            // Terminal: retrying would just fight the user or the other phone.
            Packets.REJECT_KICKED -> fail("kicked", "Disconnected from the computer.")
            Packets.REJECT_REPLACED -> fail("replaced", "Another phone connected to this computer.")
            Packets.REJECT_UNKNOWN_SESSION -> {
              if (session != 0 && parsed.session == session) {
                session = 0
                needHello = true
                emit("reconnecting")
              }
            }
            else -> {}
          }
        }
        Packets.PONG -> {
          lastPongAt = SystemClock.elapsedRealtime()
          if (parsed.payload.size >= 8) {
            val sentAt = ByteBuffer.wrap(parsed.payload).long
            rttMs = (SystemClock.elapsedRealtimeNanos() - sentAt) / 1_000_000.0
          }
        }
      }
    }
  }

  private fun send(packet: DatagramPacket) {
    try {
      socket.send(packet)
    } catch (_: Exception) {
      // Transient (no route while Wi-Fi reconnects); the watchdog handles real loss.
    }
  }

  @SuppressLint("MissingPermission") // checked in JS before start()
  private fun openRecorder(): AudioRecord? {
    val source = when {
      // Not VOICE_COMMUNICATION: outside a call (MODE_IN_COMMUNICATION) some phones
      // (e.g. Galaxy S21 FE) deliver it ~40 dB too quiet. VOICE_RECOGNITION is
      // flat; gain control, noise suppression and echo cancellation come from
      // attachEffects().
      config.voiceProcessing -> MediaRecorder.AudioSource.VOICE_RECOGNITION
      supportsUnprocessed() -> MediaRecorder.AudioSource.UNPROCESSED
      else -> MediaRecorder.AudioSource.MIC
    }
    for (rate in intArrayOf(48_000, 44_100, 16_000)) {
      val min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
      if (min <= 0) continue
      val record = try {
        AudioRecord(
          source,
          rate,
          AudioFormat.CHANNEL_IN_MONO,
          AudioFormat.ENCODING_PCM_16BIT,
          maxOf(min, rate / 100 * 2 * 8), // room for 80 ms
        )
      } catch (_: Exception) {
        continue
      }
      if (record.state == AudioRecord.STATE_INITIALIZED) {
        if (config.voiceProcessing) attachEffects(record.audioSessionId)
        return record
      }
      record.release()
    }
    return null
  }

  private fun attachEffects(audioSession: Int) {
    // VOICE_RECOGNITION is calibrated flat (90 dB SPL -> about -22 dBFS), so
    // speech arrives near -50 dBFS without gain control.
    if (AutomaticGainControl.isAvailable()) {
      AutomaticGainControl.create(audioSession)?.let { it.enabled = true; effects += it }
    }
    if (NoiseSuppressor.isAvailable()) {
      NoiseSuppressor.create(audioSession)?.let { it.enabled = true; effects += it }
    }
    if (AcousticEchoCanceler.isAvailable()) {
      AcousticEchoCanceler.create(audioSession)?.let { it.enabled = true; effects += it }
    }
  }

  private fun supportsUnprocessed(): Boolean {
    val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    return audio.getProperty(AudioManager.PROPERTY_SUPPORT_AUDIO_SOURCE_UNPROCESSED) == "true"
  }

  private fun acquireLocks() {
    // Wi-Fi power save batches packets into ~100 ms bursts; a low-latency lock avoids it.
    val wifi = context.getSystemService(Context.WIFI_SERVICE) as WifiManager
    val mode = if (Build.VERSION.SDK_INT >= 29) {
      WifiManager.WIFI_MODE_FULL_LOW_LATENCY
    } else {
      @Suppress("DEPRECATION")
      WifiManager.WIFI_MODE_FULL_HIGH_PERF
    }
    wifiLock = wifi.createWifiLock(mode, "celmic:stream").apply {
      setReferenceCounted(false)
      acquire()
    }
    val power = context.getSystemService(Context.POWER_SERVICE) as PowerManager
    wakeLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "celmic:stream").apply {
      setReferenceCounted(false)
      acquire(12 * 60 * 60 * 1000L)
    }
  }

  private fun releaseLocks() {
    runCatching { wifiLock?.release() }
    runCatching { wakeLock?.release() }
  }
}
