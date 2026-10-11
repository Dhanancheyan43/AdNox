package com.adnox.app

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.concurrent.thread

/**
 * Minimal control screen: a single button to turn protection on/off, plus status text.
 * Richer UI (per-list toggles, allowlist, live stats) is tracked in ROADMAP.md.
 */
class MainActivity : Activity() {

    private lateinit var status: TextView
    private lateinit var toggle: Button

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            setPadding(48, 48, 48, 48)
            layoutParams = ViewGroup.LayoutParams(MATCH, MATCH)
        }

        val title = TextView(this).apply {
            text = "AdNox"
            textSize = 32f
            gravity = Gravity.CENTER
        }
        status = TextView(this).apply {
            text = "Protection is OFF"
            textSize = 16f
            gravity = Gravity.CENTER
            setPadding(0, 32, 0, 32)
        }
        toggle = Button(this).apply {
            text = "Turn on"
            setOnClickListener { onToggle() }
        }
        val update = Button(this).apply {
            text = "Update filter lists"
            setOnClickListener { onUpdate() }
        }

        root.addView(title)
        root.addView(status)
        root.addView(toggle)
        root.addView(update)
        setContentView(root)
    }

    private fun onToggle() {
        val prepareIntent = VpnService.prepare(this)
        if (prepareIntent != null) {
            startActivityForResult(prepareIntent, REQ_VPN)
        } else {
            onActivityResult(REQ_VPN, RESULT_OK, null)
        }
    }

    private fun onUpdate() {
        status.text = "Updating lists…"
        thread {
            val n = FilterLists.update(this)
            runOnUiThread { status.text = "Updated $n lists. Restart protection to apply." }
        }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQ_VPN && resultCode == RESULT_OK) {
            startService(Intent(this, AdNoxVpnService::class.java))
            status.text = "Protection is ON"
            toggle.text = "Turn off"
            toggle.setOnClickListener { stopProtection() }
        }
    }

    private fun stopProtection() {
        startService(Intent(this, AdNoxVpnService::class.java).setAction(AdNoxVpnService.ACTION_STOP))
        status.text = "Protection is OFF"
        toggle.text = "Turn on"
        toggle.setOnClickListener { onToggle() }
    }

    companion object {
        private const val REQ_VPN = 1001
        private const val MATCH = ViewGroup.LayoutParams.MATCH_PARENT
    }
}
