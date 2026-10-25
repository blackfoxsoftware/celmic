package dev.celmic.audio

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Handler
import android.os.Looper
import expo.modules.kotlin.Promise
import java.net.Inet4Address

/** Browses `_celmic._udp` for [timeoutMs] and resolves with every desktop found. */
internal class Discovery(context: Context) {
  private val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
  private val found = LinkedHashMap<String, Map<String, Any?>>()
  private val pending = ArrayDeque<NsdServiceInfo>()
  private var resolving = false
  private var finished = false

  fun discover(timeoutMs: Long, promise: Promise) {
    val listener = object : NsdManager.DiscoveryListener {
      override fun onDiscoveryStarted(serviceType: String) {}
      override fun onDiscoveryStopped(serviceType: String) {}
      override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) = finish(promise)
      override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}
      override fun onServiceLost(serviceInfo: NsdServiceInfo) {}
      override fun onServiceFound(serviceInfo: NsdServiceInfo) {
        synchronized(this@Discovery) { pending.addLast(serviceInfo) }
        resolveNext()
      }
    }

    try {
      nsd.discoverServices("_celmic._udp", NsdManager.PROTOCOL_DNS_SD, listener)
    } catch (e: Exception) {
      promise.reject("E_DISCOVERY", e.message ?: "Discovery failed", e)
      return
    }
    Handler(Looper.getMainLooper()).postDelayed({
      runCatching { nsd.stopServiceDiscovery(listener) }
      finish(promise)
    }, timeoutMs)
  }

  // Older Android versions only allow one resolve at a time.
  private fun resolveNext() {
    val next = synchronized(this) {
      if (resolving || finished) return
      pending.removeFirstOrNull()?.also { resolving = true }
    } ?: return

    val listener = object : NsdManager.ResolveListener {
      override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = done()
      override fun onServiceResolved(serviceInfo: NsdServiceInfo) {
        record(serviceInfo)
        done()
      }

      private fun done() {
        synchronized(this@Discovery) { resolving = false }
        resolveNext()
      }
    }
    @Suppress("DEPRECATION")
    nsd.resolveService(next, listener)
  }

  @Suppress("DEPRECATION")
  private fun record(info: NsdServiceInfo) {
    fun attr(key: String) = info.attributes[key]?.let { String(it, Charsets.UTF_8) }

    val hosts = LinkedHashSet<String>()
    (info.host as? Inet4Address)?.hostAddress?.let(hosts::add)
    attr("ips")?.split(",")?.map { it.trim() }?.filter { it.isNotEmpty() }?.forEach(hosts::add)
    if (hosts.isEmpty()) return

    val name = attr("name") ?: info.serviceName
    synchronized(this) {
      found[name] = mapOf("name" to name, "hosts" to hosts.toList(), "port" to info.port)
    }
  }

  private fun finish(promise: Promise) {
    val result = synchronized(this) {
      if (finished) return
      finished = true
      found.values.toList()
    }
    promise.resolve(result)
  }
}
