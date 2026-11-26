package com.neplextech.oppa.mobile

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.Handler
import android.os.Looper
import java.net.Inet4Address
import java.util.ArrayDeque
import java.util.concurrent.Executors

/**
 * Browses DNS-SD service types with `NsdManager` for a bounded window and
 * resolves each instance to an address and port.
 *
 * `NsdManager` needs no multicast lock or location permission. Resolution is
 * serialized on Android 13 and older, where only one resolve may be active.
 */
internal class ServiceBrowser(context: Context) {
    private val nsd = context.applicationContext.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val handler = Handler(Looper.getMainLooper())

    data class Endpoint(val name: String, val serviceType: String, val host: String, val port: Int)

    fun browse(serviceTypes: List<String>, timeoutMs: Long, done: (List<Endpoint>) -> Unit) {
        val found = LinkedHashMap<String, Endpoint>()
        val pending = ArrayDeque<NsdServiceInfo>()
        val listeners = mutableListOf<NsdManager.DiscoveryListener>()
        var resolving = false
        var finished = false

        fun resolveNext() {
            if (finished || resolving) return
            val next = pending.poll() ?: return
            resolving = true
            resolve(next) { info ->
                handler.post {
                    resolving = false
                    if (info != null) {
                        address(info)?.let { host ->
                            val type = normalizeType(info.serviceType ?: next.serviceType)
                            found["$host:${info.port}"] = Endpoint(info.serviceName, type, host, info.port)
                        }
                    }
                    resolveNext()
                }
            }
        }

        for (type in serviceTypes) {
            val listener = object : NsdManager.DiscoveryListener {
                override fun onDiscoveryStarted(serviceType: String) {}
                override fun onDiscoveryStopped(serviceType: String) {}
                override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                    Logs.warn("discovery for $serviceType failed: $errorCode")
                }
                override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}
                override fun onServiceLost(serviceInfo: NsdServiceInfo) {}
                override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                    handler.post {
                        pending.add(serviceInfo)
                        resolveNext()
                    }
                }
            }
            try {
                nsd.discoverServices(type, NsdManager.PROTOCOL_DNS_SD, listener)
                listeners.add(listener)
            } catch (error: Exception) {
                Logs.warn("discovery for $type could not start: ${error.javaClass.simpleName}")
            }
        }

        handler.postDelayed({
            finished = true
            for (listener in listeners) {
                try {
                    nsd.stopServiceDiscovery(listener)
                } catch (_: Exception) {
                }
            }
            done(found.values.toList())
        }, timeoutMs.coerceIn(500, 30_000))
    }

    private fun resolve(info: NsdServiceInfo, result: (NsdServiceInfo?) -> Unit) {
        if (Build.VERSION.SDK_INT >= 34) {
            val executor = Executors.newSingleThreadExecutor()
            val callback = object : NsdManager.ServiceInfoCallback {
                override fun onServiceInfoCallbackRegistrationFailed(errorCode: Int) {
                    executor.shutdown()
                    result(null)
                }
                override fun onServiceUpdated(serviceInfo: NsdServiceInfo) {
                    try {
                        nsd.unregisterServiceInfoCallback(this)
                    } catch (_: Exception) {
                    }
                    result(serviceInfo)
                }
                override fun onServiceLost() {}
                override fun onServiceInfoCallbackUnregistered() {
                    executor.shutdown()
                }
            }
            try {
                nsd.registerServiceInfoCallback(info, executor, callback)
            } catch (error: Exception) {
                executor.shutdown()
                result(null)
            }
        } else {
            @Suppress("DEPRECATION")
            nsd.resolveService(info, object : NsdManager.ResolveListener {
                override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = result(null)
                override fun onServiceResolved(serviceInfo: NsdServiceInfo) = result(serviceInfo)
            })
        }
    }

    private fun address(info: NsdServiceInfo): String? {
        val addresses = if (Build.VERSION.SDK_INT >= 34) {
            info.hostAddresses
        } else {
            @Suppress("DEPRECATION")
            listOfNotNull(info.host)
        }
        val preferred = addresses.firstOrNull { it is Inet4Address } ?: addresses.firstOrNull()
        return preferred?.hostAddress?.substringBefore('%')
    }

    private fun normalizeType(type: String): String =
        type.trim('.').removeSuffix(".local").trim('.')
}
