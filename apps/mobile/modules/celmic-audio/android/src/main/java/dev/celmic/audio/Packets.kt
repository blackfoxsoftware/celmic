package dev.celmic.audio

import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Celmic wire format. See docs/PROTOCOL.md. */
internal object Packets {
  const val VERSION: Byte = 1
  const val HEADER_LEN = 20

  const val HELLO = 0x01
  const val WELCOME = 0x02
  const val REJECT = 0x03
  const val AUDIO = 0x10
  const val PING = 0x20
  const val PONG = 0x21
  const val BYE = 0x30

  const val REJECT_BAD_CODE = 1
  const val REJECT_UNKNOWN_SESSION = 2
  const val REJECT_UNSUPPORTED_VERSION = 3
  const val REJECT_KICKED = 4
  const val REJECT_REPLACED = 5

  private const val CODEC_PCM_S16LE: Byte = 0
  private val MAGIC = byteArrayOf(0x43, 0x4D, 0x49, 0x43) // "CMIC"

  class Parsed(val type: Int, val session: Int, val payload: ByteArray)

  /** Writes a header at the start of [buf] (big-endian). */
  fun writeHeader(
    buf: ByteArray,
    type: Int,
    session: Int,
    seq: Int = 0,
    sampleRate: Int = 0,
  ) {
    val b = ByteBuffer.wrap(buf, 0, HEADER_LEN).order(ByteOrder.BIG_ENDIAN)
    b.put(MAGIC)
    b.put(VERSION)
    b.put(type.toByte())
    b.put(CODEC_PCM_S16LE) // codec, only meaningful for AUDIO
    b.put((if (type == AUDIO) 1 else 0).toByte()) // channels
    b.putInt(session)
    b.putInt(seq)
    b.putInt(sampleRate)
  }

  fun build(type: Int, session: Int, payload: ByteArray = ByteArray(0)): ByteArray {
    val out = ByteArray(HEADER_LEN + payload.size)
    writeHeader(out, type, session)
    System.arraycopy(payload, 0, out, HEADER_LEN, payload.size)
    return out
  }

  fun hello(code: String, deviceName: String): ByteArray {
    val name = deviceName.toByteArray(Charsets.UTF_8).let { if (it.size > 64) it.copyOf(64) else it }
    return build(HELLO, 0, code.toByteArray(Charsets.US_ASCII) + name)
  }

  fun parse(data: ByteArray, length: Int): Parsed? {
    if (length < HEADER_LEN) return null
    for (i in 0 until 4) if (data[i] != MAGIC[i]) return null
    val b = ByteBuffer.wrap(data, 0, length).order(ByteOrder.BIG_ENDIAN)
    val type = data[5].toInt() and 0xff
    val session = b.getInt(8)
    return Parsed(type, session, data.copyOfRange(HEADER_LEN, length))
  }

  /** Encodes [count] samples of [pcm] as little-endian s16 right after the header. */
  fun writePcm(buf: ByteArray, pcm: ShortArray, count: Int) {
    var o = HEADER_LEN
    for (i in 0 until count) {
      val s = pcm[i].toInt()
      buf[o++] = (s and 0xff).toByte()
      buf[o++] = ((s shr 8) and 0xff).toByte()
    }
  }
}
