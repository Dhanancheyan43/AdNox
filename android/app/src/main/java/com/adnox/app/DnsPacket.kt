package com.adnox.app

import java.nio.ByteBuffer

/**
 * Minimal DNS message helpers: read the queried name and synthesise a sinkhole answer.
 * Only the parts AdNox needs are implemented (question parsing + A/AAAA answer synthesis).
 */
object DnsPacket {

    /** Parse the first question's name from a DNS message, or null if it cannot be read. */
    fun questionName(dns: ByteArray): String? {
        try {
            if (dns.size < 12) return null
            val qdCount = ((dns[4].toInt() and 0xFF) shl 8) or (dns[5].toInt() and 0xFF)
            if (qdCount < 1) return null
            var i = 12
            val sb = StringBuilder()
            while (i < dns.size) {
                val len = dns[i].toInt() and 0xFF
                if (len == 0) break
                // Compression pointers are not expected in the question section.
                if (len and 0xC0 != 0) return null
                i++
                if (i + len > dns.size) return null
                if (sb.isNotEmpty()) sb.append('.')
                sb.append(String(dns, i, len, Charsets.US_ASCII))
                i += len
            }
            return sb.toString().lowercase().ifEmpty { null }
        } catch (e: Exception) {
            return null
        }
    }

    /** The QTYPE of the first question (1 = A, 28 = AAAA, …), or -1. */
    fun questionType(dns: ByteArray): Int {
        var i = 12
        while (i < dns.size) {
            val len = dns[i].toInt() and 0xFF
            if (len == 0) { i++; break }
            if (len and 0xC0 != 0) return -1
            i += 1 + len
        }
        if (i + 2 > dns.size) return -1
        return ((dns[i].toInt() and 0xFF) shl 8) or (dns[i + 1].toInt() and 0xFF)
    }

    /**
     * Build a sinkhole response for [query]: a NOERROR answer of `0.0.0.0` (A) or `::` (AAAA),
     * or NODATA for other types. Returns null if the query cannot be parsed.
     */
    fun sinkhole(query: ByteArray, ttl: Int = 60): ByteArray? {
        if (query.size < 12) return null
        // Find end of the question section (header + QNAME + QTYPE + QCLASS).
        var i = 12
        while (i < query.size) {
            val len = query[i].toInt() and 0xFF
            if (len == 0) { i++; break }
            if (len and 0xC0 != 0) return null
            i += 1 + len
        }
        val qEnd = i + 4 // QTYPE(2) + QCLASS(2)
        if (qEnd > query.size) return null

        val qtype = ((query[i].toInt() and 0xFF) shl 8) or (query[i + 1].toInt() and 0xFF)
        val answerRdata: ByteArray? = when (qtype) {
            1 -> ByteArray(4)   // A    -> 0.0.0.0
            28 -> ByteArray(16) // AAAA -> ::
            else -> null        // NODATA
        }
        val ancount = if (answerRdata != null) 1 else 0

        val size = qEnd + if (answerRdata != null) 12 + answerRdata.size else 0
        val out = ByteBuffer.allocate(size)
        out.put(query, 0, 2)                       // ID
        out.put(0x81.toByte())                     // QR=1, Opcode=0, AA=0, TC=0, RD=1
        out.put(0x80.toByte())                     // RA=1, RCODE=0 (NOERROR)
        out.putShort(1)                            // QDCOUNT
        out.putShort(ancount.toShort())            // ANCOUNT
        out.putShort(0)                            // NSCOUNT
        out.putShort(0)                            // ARCOUNT
        out.put(query, 12, qEnd - 12)              // original question

        if (answerRdata != null) {
            out.putShort(0xC00C.toShort())         // name pointer to offset 12
            out.putShort(qtype.toShort())          // TYPE
            out.putShort(1)                        // CLASS IN
            out.putInt(ttl)                        // TTL
            out.putShort(answerRdata.size.toShort())
            out.put(answerRdata)
        }
        return out.array()
    }
}
