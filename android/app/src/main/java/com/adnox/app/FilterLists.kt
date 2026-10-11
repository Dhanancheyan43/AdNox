package com.adnox.app

import android.content.Context
import java.io.File

/**
 * Manages the on-device filter lists. Lists live under the app's files dir in `lists/`, plus a
 * bundled seed list shipped in assets so AdNox blocks something on first launch before any
 * download. The UI's "Update" action downloads the full lists (StevenBlack, AdGuard DNS, …)
 * into that directory; see ROADMAP.md for the scheduled-update worker.
 */
object FilterLists {

    private const val SEED_ASSET = "adnox-seed.txt"

    /** Default sources, mirrored from the CLI's defaults. */
    val DEFAULT_SOURCES = listOf(
        "stevenblack-hosts.txt" to "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts",
        "peter-lowe.txt" to "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=hosts&showintro=0&mimetype=plaintext",
        "adguard-dns.txt" to "https://raw.githubusercontent.com/AdguardTeam/FiltersRegistry/master/filters/filter_15_DnsFilter/filter.txt",
    )

    fun listsDir(ctx: Context): File = File(ctx.filesDir, "lists").apply { mkdirs() }

    /** Combined text of the seed list plus every downloaded list and the user allowlist. */
    fun loadCombined(ctx: Context): String {
        val sb = StringBuilder()
        // Bundled seed (always present).
        try {
            ctx.assets.open(SEED_ASSET).use { sb.append(String(it.readBytes())).append('\n') }
        } catch (_: Exception) {}
        // Downloaded lists.
        listsDir(ctx).listFiles()?.sortedBy { it.name }?.forEach { f ->
            if (f.isFile) runCatching { sb.append(f.readText()).append('\n') }
        }
        return sb.toString()
    }

    /** Download the default lists into the lists dir. Call off the main thread. */
    fun update(ctx: Context): Int {
        var ok = 0
        val dir = listsDir(ctx)
        for ((name, url) in DEFAULT_SOURCES) {
            runCatching {
                val conn = java.net.URL(url).openConnection() as java.net.HttpURLConnection
                conn.connectTimeout = 15000
                conn.readTimeout = 60000
                conn.setRequestProperty("User-Agent", "AdNox/0.1")
                conn.inputStream.use { input ->
                    File(dir, name).outputStream().use { input.copyTo(it) }
                }
                ok++
            }
        }
        return ok
    }
}
