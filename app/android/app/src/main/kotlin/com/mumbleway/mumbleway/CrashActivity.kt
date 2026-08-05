package com.mumbleway.mumbleway

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.os.Bundle
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast

/**
 * Shows what went wrong, instead of the app just vanishing.
 *
 * Runs in its own process, declared in the manifest. The process that crashed
 * is on its way out and may be in any state at all — a heap that could not
 * allocate, a Flutter engine half torn down — so building an interface inside
 * it is asking the broken thing to draw its own error message. A separate
 * process starts clean.
 *
 * Built in code rather than from a layout resource for the same reason: fewer
 * things to load, and no dependency on resources that a packaging fault might
 * be the very thing that broke.
 */
class CrashActivity : Activity() {

    companion object {
        const val EXTRA_REPORT = "report"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val report = intent.getStringExtra(EXTRA_REPORT)
            ?: CrashReporter.pending(this)
            ?: "No details were recorded."

        // The first line is the reason; the rest is the trace. Shown apart so
        // the answer to "what happened" does not need scrolling to.
        val reason = report.lineSequence().firstOrNull().orEmpty()

        val pad = dp(20)
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#101822"))
            setPadding(pad, dp(36), pad, pad)
        }

        root.addView(
            TextView(this).apply {
                text = "MumbleWay stopped"
                setTextColor(Color.WHITE)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 22f)
                setTypeface(typeface, Typeface.BOLD)
            },
        )

        root.addView(
            TextView(this).apply {
                text = reason
                setTextColor(Color.parseColor("#FFB4A9"))
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                setPadding(0, dp(10), 0, dp(4))
            },
        )

        root.addView(
            TextView(this).apply {
                text = "The details below say where. Copy them into a bug " +
                    "report — they are the whole of what is known."
                setTextColor(Color.parseColor("#9AA6B2"))
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
                setPadding(0, 0, 0, dp(12))
            },
        )

        val trace = TextView(this).apply {
            text = report
            setTextColor(Color.parseColor("#C7D0DA"))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
            typeface = Typeface.MONOSPACE
            setTextIsSelectable(true)
        }

        root.addView(
            ScrollView(this).apply {
                addView(
                    // Horizontally too: a stack trace is wide, and wrapping it
                    // turns one frame per line into an unreadable block.
                    android.widget.HorizontalScrollView(this@CrashActivity).apply {
                        addView(trace)
                    },
                )
            },
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                0,
            ).apply { weight = 1f },
        )

        val buttons = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END
            setPadding(0, dp(12), 0, 0)
        }
        buttons.addView(
            Button(this).apply {
                text = "Copy"
                setOnClickListener { copy(report) }
            },
        )
        buttons.addView(
            Button(this).apply {
                text = "Close"
                setOnClickListener {
                    CrashReporter.clear(this@CrashActivity)
                    finish()
                }
            },
        )
        root.addView(buttons)

        setContentView(root)
    }

    /**
     * The report is forgotten once it has actually reached the screen.
     *
     * Not in `onCreate`: an activity started from a process that is in the
     * middle of dying does not always survive to be drawn — the task it was
     * put in can be torn down with the process that asked for it — and
     * clearing the file there threw the report away in exactly the case it was
     * being kept for. By `onResume` it is on screen and being read.
     */
    override fun onResume() {
        super.onResume()
        CrashReporter.clear(this)
    }

    private fun copy(report: String) {
        val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        clipboard.setPrimaryClip(ClipData.newPlainText("MumbleWay crash", report))
        Toast.makeText(this, "Copied", Toast.LENGTH_SHORT).show()
    }

    private fun dp(value: Int): Int =
        (value * resources.displayMetrics.density).toInt()

    @Suppress("UNUSED_PARAMETER")
    private fun unused(v: View) = Unit

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
    }
}
