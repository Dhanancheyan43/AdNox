package com.adnox.app

import java.nio.ByteBuffer

/**
 * Just enough IPv4 + UDP packet handling to filter DNS inside the VpnService tun loop.
 *
 * Scope for v1: IPv4 + UDP. IPv6 and DNS-over-TCP are tracked in ROADMAP.md.
 */
object IpUtils {

    data class UdpDatagram(
        val srcAddr: ByteArray,
        val dstAddr: ByteArray,
        val srcPort: Int,
        val dstPort: Int,
        val payload: ByteArray,
    )

    /** Parse an IPv4/UDP packet, or null if it is not IPv4+UDP. */
    fun parseIpv4Udp(packet: ByteArray, length: Int): UdpDatagram? {
        if (length < 28) return null
        val version = (packet[0].toInt() and 0xF0) ushr 4
        if (version != 4) return null
        val ihl = (packet[0].toInt() and 0x0F) * 4
        if (ihl < 20 || length < ihl + 8) return null
        val protocol = packet[9].toInt() and 0xFF
        if (protocol != 17) return null // UDP

        val srcAddr = packet.copyOfRange(12, 16)
        val dstAddr = packet.copyOfRange(16, 20)
        val srcPort = u16(packet, ihl)
        val dstPort = u16(packet, ihl + 2)
        val udpLen = u16(packet, ihl + 4)
        val payloadLen = (udpLen - 8).coerceAtLeast(0)
        val payloadStart = ihl + 8
        val payloadEnd = (payloadStart + payloadLen).coerceAtMost(length)
        val payload = packet.copyOfRange(payloadStart, payloadEnd)
        return UdpDatagram(srcAddr, dstAddr, srcPort, dstPort, payload)
    }

    /**
     * Build an IPv4/UDP packet. Addresses/ports are given from the sender's point of view, so
     * to reply to a captured query swap src/dst when calling this.
     */
    fun buildIpv4Udp(
        srcAddr: ByteArray,
        dstAddr: ByteArray,
        srcPort: Int,
        dstPort: Int,
        payload: ByteArray,
    ): ByteArray {
        val totalLen = 20 + 8 + payload.size
        val buf = ByteBuffer.allocate(totalLen)

        // --- IPv4 header ---
        buf.put(0x45)                       // version 4, IHL 5
        buf.put(0)                          // DSCP/ECN
        buf.putShort(totalLen.toShort())    // total length
        buf.putShort(0)                     // identification
        buf.putShort(0x4000.toShort())      // flags: don't fragment
        buf.put(64)                         // TTL
        buf.put(17)                         // protocol UDP
        buf.putShort(0)                     // header checksum (filled below)
        buf.put(srcAddr)
        buf.put(dstAddr)

        // --- UDP header ---
        val udpLen = 8 + payload.size
        buf.putShort(srcPort.toShort())
        buf.putShort(dstPort.toShort())
        buf.putShort(udpLen.toShort())
        buf.putShort(0)                     // UDP checksum (filled below)
        buf.put(payload)

        val bytes = buf.array()
        // IPv4 header checksum.
        writeU16(bytes, 10, checksum(bytes, 0, 20))
        // UDP checksum with pseudo-header.
        writeU16(bytes, 26, udpChecksum(bytes, srcAddr, dstAddr, 20, udpLen))
        return bytes
    }

    private fun u16(b: ByteArray, off: Int): Int =
        ((b[off].toInt() and 0xFF) shl 8) or (b[off + 1].toInt() and 0xFF)

    private fun writeU16(b: ByteArray, off: Int, v: Int) {
        b[off] = ((v ushr 8) and 0xFF).toByte()
        b[off + 1] = (v and 0xFF).toByte()
    }

    private fun checksum(data: ByteArray, off: Int, len: Int): Int {
        var sum = 0L
        var i = off
        val end = off + len
        while (i + 1 < end) {
            sum += (((data[i].toInt() and 0xFF) shl 8) or (data[i + 1].toInt() and 0xFF)).toLong()
            i += 2
        }
        if (i < end) sum += ((data[i].toInt() and 0xFF) shl 8).toLong()
        while (sum shr 16 != 0L) sum = (sum and 0xFFFF) + (sum shr 16)
        return (sum.inv() and 0xFFFF).toInt()
    }

    private fun udpChecksum(
        packet: ByteArray,
        srcAddr: ByteArray,
        dstAddr: ByteArray,
        udpOff: Int,
        udpLen: Int,
    ): Int {
        var sum = 0L
        // Pseudo-header: src(4) + dst(4) + zero(1) + proto(1) + udpLen(2)
        for (a in listOf(srcAddr, dstAddr)) {
            sum += (((a[0].toInt() and 0xFF) shl 8) or (a[1].toInt() and 0xFF)).toLong()
            sum += (((a[2].toInt() and 0xFF) shl 8) or (a[3].toInt() and 0xFF)).toLong()
        }
        sum += 17L            // protocol
        sum += udpLen.toLong()
        var i = udpOff
        val end = udpOff + udpLen
        while (i + 1 < end) {
            sum += (((packet[i].toInt() and 0xFF) shl 8) or (packet[i + 1].toInt() and 0xFF)).toLong()
            i += 2
        }
        if (i < end) sum += ((packet[i].toInt() and 0xFF) shl 8).toLong()
        while (sum shr 16 != 0L) sum = (sum and 0xFFFF) + (sum shr 16)
        val cs = (sum.inv() and 0xFFFF).toInt()
        return if (cs == 0) 0xFFFF else cs
    }
}
