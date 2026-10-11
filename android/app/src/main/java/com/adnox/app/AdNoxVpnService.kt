package com.adnox.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.util.Log
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.InetSocketAddress
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/**
 * AdNox VpnService.
 *
 * Establishes a local-only VPN tun, declares itself the DNS server for the device, and filters
 * every DNS query: blocked domains get an immediate `0.0.0.0` sinkhole answer, everything else is
 * forwarded to the upstream resolver through a protected socket (so forwarding does not loop back
 * into the VPN). Because this intercepts DNS, it filters ads/trackers for *every app* — no root
 * and no certificate required.
 *
 * v1 handles IPv4 + UDP DNS. IPv6 and DNS-over-TCP/DoH handling are tracked in ROADMAP.md.
 */
class AdNoxVpnService : VpnService() {

    @Volatile private var tun: ParcelFileDescriptor? = null
    private var worker: Thread? = null
    private val running = AtomicBoolean(false)

    // Simple shared stats (exposed to the UI via a bound binder / broadcast in a later phase).
    private val blocked = AtomicLong(0)
    private val forwarded = AtomicLong(0)

    private lateinit var engine: NativeEngine

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                stopVpn()
                return START_NOT_STICKY
            }
            else -> startVpn()
        }
        return START_STICKY
    }

    private fun startVpn() {
        if (running.getAndSet(true)) return

        // Build the engine from the lists persisted by the UI (see FilterLists in the app).
        val rules = FilterLists.loadCombined(this)
        engine = NativeEngine.fromRules(rules)
        Log.i(TAG, "engine loaded: ${engine.domainCount()} domains")

        startForeground(NOTIF_ID, buildNotification())

        val builder = Builder()
            .setSession("AdNox")
            .addAddress(VPN_ADDR, 32)
            .addDnsServer(DNS_SENTINEL)
            // Route only the sentinel DNS address through the tun; other traffic is untouched.
            .addRoute(DNS_SENTINEL, 32)
            .setBlocking(true)
        // Keep AdNox itself out of the tunnel so upstream forwarding works.
        try {
            builder.addDisallowedApplication(packageName)
        } catch (_: Exception) { /* own package always resolvable */ }

        val pfd = builder.establish() ?: run {
            Log.e(TAG, "failed to establish VPN")
            stopVpn()
            return
        }
        tun = pfd

        worker = Thread({ loop(pfd) }, "adnox-vpn").also { it.start() }
    }

    private fun loop(pfd: ParcelFileDescriptor) {
        val input = FileInputStream(pfd.fileDescriptor)
        val output = FileOutputStream(pfd.fileDescriptor)
        val packet = ByteArray(32767)

        while (running.get()) {
            val len = try {
                input.read(packet)
            } catch (e: Exception) {
                if (running.get()) Log.w(TAG, "read error", e)
                break
            }
            if (len <= 0) continue

            val dgram = IpUtils.parseIpv4Udp(packet, len) ?: continue
            if (dgram.dstPort != 53) continue
            val dns = dgram.payload
            val name = DnsPacket.questionName(dns)

            if (name != null && engine.isBlockedDomain(name)) {
                val answer = DnsPacket.sinkhole(dns) ?: continue
                // Reply from the sentinel DNS address back to the app.
                val reply = IpUtils.buildIpv4Udp(
                    srcAddr = dgram.dstAddr,
                    dstAddr = dgram.srcAddr,
                    srcPort = dgram.dstPort,
                    dstPort = dgram.srcPort,
                    payload = answer,
                )
                output.write(reply)
                blocked.incrementAndGet()
            } else {
                forwardUpstream(dns)?.let { upstreamResp ->
                    val reply = IpUtils.buildIpv4Udp(
                        srcAddr = dgram.dstAddr,
                        dstAddr = dgram.srcAddr,
                        srcPort = dgram.dstPort,
                        dstPort = dgram.srcPort,
                        payload = upstreamResp,
                    )
                    output.write(reply)
                    forwarded.incrementAndGet()
                }
            }
        }
    }

    /** Forward a raw DNS query to the real upstream resolver via a protected socket. */
    private fun forwardUpstream(query: ByteArray): ByteArray? {
        return try {
            java.net.DatagramSocket().use { sock ->
                // protect() keeps this socket outside the VPN so it reaches the real resolver.
                protect(sock)
                sock.soTimeout = 5000
                val dest = InetSocketAddress(UPSTREAM_DNS, 53)
                sock.send(java.net.DatagramPacket(query, query.size, dest))
                val buf = ByteArray(1500)
                val resp = java.net.DatagramPacket(buf, buf.size)
                sock.receive(resp)
                buf.copyOf(resp.length)
            }
        } catch (e: Exception) {
            Log.w(TAG, "upstream forward failed", e)
            null
        }
    }

    private fun stopVpn() {
        running.set(false)
        try {
            tun?.close()
        } catch (_: Exception) {}
        tun = null
        worker = null
        if (this::engine.isInitialized) engine.close()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        stopVpn()
        super.onDestroy()
    }

    private fun buildNotification(): Notification {
        val mgr = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            mgr.createNotificationChannel(
                NotificationChannel(CHANNEL, "AdNox", NotificationManager.IMPORTANCE_LOW)
            )
        }
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE
        )
        return Notification.Builder(this, CHANNEL)
            .setContentTitle("AdNox is protecting you")
            .setContentText("Blocking ads & trackers")
            .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
            .setContentIntent(open)
            .setOngoing(true)
            .build()
    }

    companion object {
        private const val TAG = "AdNoxVpn"
        const val ACTION_STOP = "com.adnox.app.STOP"
        private const val NOTIF_ID = 1
        private const val CHANNEL = "adnox"

        private const val VPN_ADDR = "10.111.0.1"
        private const val DNS_SENTINEL = "10.111.0.2"
        private const val UPSTREAM_DNS = "1.1.1.1"
    }
}
