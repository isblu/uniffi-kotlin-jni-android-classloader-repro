package dev.isblu.uniffijnirepro

import android.app.Activity
import android.os.Bundle
import android.util.Log
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import uniffi.repro.Listener
import uniffi.repro.fireFromRustThread
import uniffi.repro.fireOnCallingThread
import uniffi.repro.runtimeFlavor

private const val TAG = "Repro"

class MainActivity : Activity() {
    private lateinit var output: TextView

    private val listener = object : Listener {
        override fun onEvent(n: UInt) {
            log("Kotlin Listener.onEvent($n) on thread '${Thread.currentThread().name}'")
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        output = TextView(this).apply { setTextIsSelectable(true); textSize = 13f }
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 96, 32, 32)
            addView(TextView(this@MainActivity).apply {
                text = "Runtime: ${runtimeFlavor()}\n\n" +
                    "Tap (1) FIRST after a fresh app start to reproduce. Tapping (2) first " +
                    "caches the callback method on the main thread and hides the bug."
            })
            addView(Button(this@MainActivity).apply {
                text = "1) Fire from Rust thread first"
                setOnClickListener { log("result: " + fireFromRustThread(listener)) }
            })
            addView(Button(this@MainActivity).apply {
                text = "2) Fire on main thread"
                setOnClickListener { log("result: " + fireOnCallingThread(listener)) }
            })
            addView(output)
        }
        setContentView(ScrollView(this).apply { addView(root) })
        log("started, runtime = ${runtimeFlavor()}")
    }

    private fun log(msg: String) {
        Log.i(TAG, msg)
        runOnUiThread { output.append(msg + "\n\n") }
    }
}
