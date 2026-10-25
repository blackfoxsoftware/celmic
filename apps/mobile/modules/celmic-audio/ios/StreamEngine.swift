import AVFoundation
import Foundation
import Network

struct StreamConfig {
  let hosts: [String]
  let port: UInt16
  let code: String
  let deviceName: String
  let voiceProcessing: Bool
}

/// Wire format. See docs/PROTOCOL.md.
enum Packets {
  static let headerLength = 20
  static let hello: UInt8 = 0x01
  static let welcome: UInt8 = 0x02
  static let reject: UInt8 = 0x03
  static let audio: UInt8 = 0x10
  static let ping: UInt8 = 0x20
  static let pong: UInt8 = 0x21
  static let bye: UInt8 = 0x30

  static func build(type: UInt8, session: UInt32, seq: UInt32 = 0, sampleRate: UInt32 = 0, payload: Data = Data()) -> Data {
    var data = Data(capacity: headerLength + payload.count)
    data.append(contentsOf: [0x43, 0x4D, 0x49, 0x43, 1, type, 0, type == audio ? 1 : 0])
    withUnsafeBytes(of: session.bigEndian) { data.append(contentsOf: $0) }
    withUnsafeBytes(of: seq.bigEndian) { data.append(contentsOf: $0) }
    withUnsafeBytes(of: sampleRate.bigEndian) { data.append(contentsOf: $0) }
    data.append(payload)
    return data
  }

  /// Returns (type, session, payload) for a valid Celmic datagram.
  static func parse(_ data: Data) -> (UInt8, UInt32, Data)? {
    let bytes = [UInt8](data)
    guard bytes.count >= headerLength, bytes[0...3] == [0x43, 0x4D, 0x49, 0x43] else { return nil }
    let session = bytes[8...11].reduce(UInt32(0)) { $0 << 8 | UInt32($1) }
    return (bytes[5], session, Data(bytes[headerLength...]))
  }
}

/// Process-wide owner of the single active stream.
final class StreamEngine {
  static let shared = StreamEngine()

  var onState: (([String: Any?]) -> Void)?
  var onStats: (([String: Any?]) -> Void)?
  private(set) var lastState: [String: Any?] = ["state": "idle"]
  var muted = false

  private var session: StreamSession?
  private let lock = NSLock()

  func start(config: StreamConfig) {
    let next = StreamSession(config: config, engine: self)
    lock.lock()
    let previous = session
    session = next
    lock.unlock()
    // Outside the lock: stopping waits on the session queue, which may call ended().
    previous?.stop()
    muted = false
    next.start()
  }

  func stop() {
    lock.lock()
    let current = session
    session = nil
    lock.unlock()
    guard let current else { return }
    current.stop()
    emitState(["state": "idle"])
  }

  func emitState(_ state: [String: Any?]) {
    lastState = state
    onState?(state)
  }

  func emitStats(_ stats: [String: Any?]) {
    onStats?(stats)
  }

  func ended(_ ended: StreamSession) {
    lock.lock()
    if session === ended { session = nil }
    lock.unlock()
  }
}

/// One connection to one desktop: AVAudioEngine capture → 10 ms PCM frames → UDP.
final class StreamSession {
  private let config: StreamConfig
  private unowned let engine: StreamEngine
  private let queue = DispatchQueue(label: "dev.celmic.stream", qos: .userInteractive)

  private let audioEngine = AVAudioEngine()
  private var sink: AVAudioSinkNode?
  private var candidates: [NWConnection] = []
  private var connection: NWConnection?
  private var timer: DispatchSourceTimer?
  /// Drains captured audio into packets every few ms. Polling from `queue`
  /// keeps the realtime audio thread free of dispatch calls.
  private var flushTimer: DispatchSourceTimer?

  // Only touched on `queue`.
  private var running = false
  private var session: UInt32 = 0
  private var needHello = true
  private var everConnected = false
  private var startedAt = Date()
  private var lastPongAt = Date()
  private var lastPingAt = Date.distantPast
  private var rttMs: Double = -1
  private var desktopName: String?
  private var seq: UInt32 = 0
  private var sent: Double = 0

  // Touched from the realtime audio thread; guarded by `pcmLock`.
  private let pcmLock = NSLock()
  private var pending: [Int16] = []
  private var peak: Float = 0
  private var sampleRate: Double = 48_000

  init(config: StreamConfig, engine: StreamEngine) {
    self.config = config
    self.engine = engine
  }

  func start() {
    queue.async { [self] in
      running = true
      startedAt = Date()
      emit("connecting")
      do {
        try startAudio()
      } catch {
        fail("mic", "Could not start the microphone: \(error.localizedDescription)")
        return
      }
      openCandidates()
      startTimer()
      startFlushTimer()
    }
  }

  func stop() {
    queue.sync { [self] in
      guard running else { return }
      running = false
      teardown(sendBye: true)
    }
  }

  // MARK: - State

  private func emit(_ state: String) {
    guard running else { return }
    engine.emitState(["state": state, "desktopName": desktopName])
  }

  private func fail(_ reason: String, _ message: String) {
    guard running else { return }
    engine.emitState(["state": "error", "reason": reason, "error": message, "desktopName": desktopName])
    running = false
    teardown(sendBye: false)
    engine.ended(self)
  }

  private func teardown(sendBye: Bool) {
    timer?.cancel()
    timer = nil
    flushTimer?.cancel()
    flushTimer = nil
    NotificationCenter.default.removeObserver(self)
    audioEngine.stop()
    if let sink { audioEngine.detach(sink) }
    sink = nil
    if sendBye, session != 0, let connection {
      connection.send(content: Packets.build(type: Packets.bye, session: session), completion: .idempotent)
    }
    let all = candidates + [connection].compactMap { $0 }
    // Give BYE a moment to leave before cancelling.
    queue.asyncAfter(deadline: .now() + 0.1) { all.forEach { $0.cancel() } }
    candidates = []
    connection = nil
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
  }

  // MARK: - Audio

  private func startAudio() throws {
    let audioSession = AVAudioSession.sharedInstance()
    try audioSession.setCategory(
      .playAndRecord,
      mode: config.voiceProcessing ? .voiceChat : .measurement,
      options: [.mixWithOthers, .allowBluetooth]
    )
    try? audioSession.setPreferredSampleRate(48_000)
    try? audioSession.setPreferredIOBufferDuration(0.005)
    try audioSession.setActive(true)

    let input = audioEngine.inputNode
    if config.voiceProcessing {
      try? input.setVoiceProcessingEnabled(true)
    }
    let format = input.outputFormat(forBus: 0)
    guard format.sampleRate > 0, format.channelCount > 0 else {
      throw NSError(domain: "celmic", code: 1, userInfo: [NSLocalizedDescriptionKey: "No microphone input"])
    }

    pcmLock.lock()
    sampleRate = format.sampleRate
    pending.removeAll(keepingCapacity: true)
    // Reserved up front so the realtime thread never reallocates.
    pending.reserveCapacity(Int(format.sampleRate / 2))
    pcmLock.unlock()

    // AVAudioSinkNode gets buffers at the IO cycle size (~5 ms) instead of the
    // ~100 ms chunks an installTap delivers, which is what keeps latency low.
    let sink = AVAudioSinkNode { [weak self] _, frameCount, audioBufferList in
      self?.capture(audioBufferList, frames: Int(frameCount))
      return noErr
    }
    self.sink = sink
    audioEngine.attach(sink)
    audioEngine.connect(input, to: sink, format: nil)
    audioEngine.prepare()
    try audioEngine.start()

    NotificationCenter.default.addObserver(
      self, selector: #selector(audioInterrupted(_:)),
      name: AVAudioSession.interruptionNotification, object: audioSession)
    NotificationCenter.default.addObserver(
      self, selector: #selector(engineConfigurationChanged(_:)),
      name: .AVAudioEngineConfigurationChange, object: audioEngine)
  }

  /// Realtime thread: take channel 0, convert to s16, accumulate.
  private func capture(_ list: UnsafePointer<AudioBufferList>, frames: Int) {
    let buffers = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: list))
    guard let first = buffers.first, let raw = first.mData else { return }
    let samples = raw.assumingMemoryBound(to: Float.self)
    let stride = Int(first.mNumberChannels) // interleaved if > 1
    let muted = engine.muted

    pcmLock.lock()
    // If the network queue stalls, drop the oldest audio rather than grow.
    let room = pending.capacity - pending.count
    if frames > room {
      pending.removeFirst(min(pending.count, frames - room))
    }
    for i in 0..<frames {
      let value = samples[i * max(stride, 1)]
      peak = max(peak, abs(value))
      let clamped = muted ? 0 : max(-1, min(1, value))
      pending.append(Int16(clamped * 32767))
    }
    pcmLock.unlock()
  }

  /// Sends every complete 10 ms frame accumulated so far.
  private func flush() {
    guard running else { return }
    pcmLock.lock()
    let rate = sampleRate
    let frame = Int(rate / 100)
    var frames: [[Int16]] = []
    while pending.count >= frame {
      frames.append(Array(pending[0..<frame]))
      pending.removeFirst(frame)
    }
    pcmLock.unlock()

    guard session != 0, let connection, !frames.isEmpty else { return }
    for samples in frames {
      let payload = samples.withUnsafeBufferPointer { Data(buffer: $0) }
      // Int16 is little-endian on every iOS device, which matches the wire format.
      let packet = Packets.build(type: Packets.audio, session: session, seq: seq, sampleRate: UInt32(rate), payload: payload)
      seq &+= 1
      sent += 1
      connection.send(content: packet, completion: .idempotent)
    }
  }

  @objc private func audioInterrupted(_ note: Notification) {
    guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
          AVAudioSession.InterruptionType(rawValue: raw) == .ended else { return }
    queue.async { [self] in restartAudio() }
  }

  @objc private func engineConfigurationChanged(_ note: Notification) {
    // Route changed (headset plugged, Bluetooth connected): the input format may differ.
    queue.async { [self] in restartAudio() }
  }

  private func restartAudio() {
    guard running else { return }
    audioEngine.stop()
    if let sink { audioEngine.detach(sink) }
    sink = nil
    NotificationCenter.default.removeObserver(self)
    do {
      try startAudio()
    } catch {
      fail("mic", "Microphone stopped: \(error.localizedDescription)")
    }
  }

  // MARK: - Network

  private func openCandidates() {
    for host in config.hosts {
      guard let port = NWEndpoint.Port(rawValue: config.port) else { continue }
      let conn = NWConnection(host: NWEndpoint.Host(host), port: port, using: .udp)
      conn.stateUpdateHandler = { _ in }
      conn.start(queue: queue)
      receive(on: conn)
      candidates.append(conn)
    }
    if candidates.isEmpty {
      fail("network", "No valid computer address.")
    }
  }

  private func receive(on conn: NWConnection) {
    conn.receiveMessage { [weak self] data, _, _, error in
      guard let self, self.running else { return }
      if let data, let packet = Packets.parse(data) {
        self.handle(type: packet.0, session: packet.1, payload: packet.2, from: conn)
      }
      if error == nil { self.receive(on: conn) }
    }
  }

  private func handle(type: UInt8, session id: UInt32, payload: Data, from conn: NWConnection) {
    switch type {
    case Packets.welcome:
      guard id != 0 else { return }
      let fresh = needHello || session != id
      desktopName = String(data: payload, encoding: .utf8)
      session = id
      lastPongAt = Date()
      needHello = false
      everConnected = true
      if connection !== conn {
        connection = conn
        // The race is won: drop the other candidate addresses.
        candidates.filter { $0 !== conn }.forEach { $0.cancel() }
        candidates = [conn]
      }
      if fresh { emit("streaming") }

    case Packets.reject:
      switch payload.first ?? 0 {
      case 1: fail("bad_code", "Wrong pairing code.")
      case 3: fail("unsupported_version", "This computer runs an incompatible Celmic version.")
      // Terminal: retrying would just fight the user or the other phone.
      case 4: fail("kicked", "Disconnected from the computer.")
      case 5: fail("replaced", "Another phone connected to this computer.")
      case 2 where session != 0 && id == session:
        session = 0
        needHello = true
        emit("reconnecting")
      default: break
      }

    case Packets.pong:
      lastPongAt = Date()
      if payload.count >= 8 {
        let sentAt = payload.prefix(8).reduce(UInt64(0)) { $0 << 8 | UInt64($1) }
        rttMs = Double(DispatchTime.now().uptimeNanoseconds &- sentAt) / 1_000_000
      }

    default:
      break
    }
  }

  private func startFlushTimer() {
    let timer = DispatchSource.makeTimerSource(queue: queue)
    timer.schedule(deadline: .now(), repeating: .milliseconds(5), leeway: .milliseconds(1))
    timer.setEventHandler { [weak self] in self?.flush() }
    timer.resume()
    flushTimer = timer
  }

  /// 20 Hz housekeeping: HELLO retries, PING, watchdog, UI stats.
  private func startTimer() {
    let timer = DispatchSource.makeTimerSource(queue: queue)
    timer.schedule(deadline: .now(), repeating: .milliseconds(50))
    var ticks = 0
    timer.setEventHandler { [weak self] in
      guard let self, self.running else { return }
      ticks += 1
      let now = Date()

      if self.session != 0, !self.needHello, now.timeIntervalSince(self.lastPongAt) > 4 {
        self.needHello = true
        self.emit("reconnecting")
      }

      if self.needHello {
        if !self.everConnected, now.timeIntervalSince(self.startedAt) > 8 {
          self.fail("timeout", "No answer from the computer. Is Celmic open on the same network?")
          return
        }
        if ticks % 6 == 0 { // every 300 ms
          let hello = Packets.build(
            type: Packets.hello, session: 0,
            payload: Data(self.config.code.utf8) + Data(self.config.deviceName.utf8.prefix(64)))
          self.candidates.forEach { $0.send(content: hello, completion: .idempotent) }
        }
      }

      if self.session != 0, let connection = self.connection, now.timeIntervalSince(self.lastPingAt) >= 1 {
        self.lastPingAt = now
        let stamp = withUnsafeBytes(of: DispatchTime.now().uptimeNanoseconds.bigEndian) { Data($0) }
        let ping = Packets.build(type: Packets.ping, session: self.session, payload: stamp)
        connection.send(content: ping, completion: .idempotent)
      }

      if ticks % 2 == 0 {
        self.pcmLock.lock()
        let level = self.peak
        self.peak = 0
        let rate = self.sampleRate
        self.pcmLock.unlock()
        self.engine.emitStats([
          "level": Double(level),
          "rttMs": self.rttMs,
          "packetsSent": self.sent,
          "sampleRate": rate,
        ])
      }
    }
    timer.resume()
    self.timer = timer
  }
}
