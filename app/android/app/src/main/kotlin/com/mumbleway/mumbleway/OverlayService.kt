package com.mumbleway.mumbleway

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.drawable.GradientDrawable
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.view.KeyEvent
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.math.abs

/**
 * The floating push-to-talk island.
 *
 * On a motorcycle the phone is normally running a navigation app, so MumbleWay
 * spends most of its life in the background. An overlay window is the only way
 * on Android to keep the talk button reachable without switching apps, and a
 * foreground service is what keeps this process (and the audio engine) alive
 * while another app is in front.
 *
 * The window is deliberately small and draggable: it has to coexist with turn
 * instructions rather than cover them.
 */
class OverlayService : Service() {

    private var windowManager: WindowManager? = null
    private var mediaSession: MediaSession? = null
    private var root: LinearLayout? = null
    private var talkButton: TextView? = null
    private var namesView: TextView? = null
    private var muteButton: TextView? = null
    private var deafenButton: TextView? = null
    private var meterView: MeterView? = null
    private var dotView: DotView? = null
    private val flashHandler = Handler(Looper.getMainLooper())
    private var flashing = false
    private lateinit var layoutParams: WindowManager.LayoutParams

    companion object {
        private const val CHANNEL_ID = "mumbleway_overlay"
        private const val NOTIFICATION_ID = 4711

        const val ACTION_START = "com.mumbleway.overlay.START"
        const val ACTION_STOP = "com.mumbleway.overlay.STOP"

        /** Set by [MainActivity] so the button can reach the Flutter engine. */
        @Volatile
        var onTransmit: ((Boolean) -> Unit)? = null

        /**
         * The remaining island controls. These toggle rather than set: the
         * island is a second view onto the app's state, never a second copy of
         * it, so it asks for a change and waits to be told the new value.
         */
        @Volatile
        var onToggleMute: (() -> Unit)? = null

        @Volatile
        var onToggleDeafen: (() -> Unit)? = null

        @Volatile
        var onHangup: (() -> Unit)? = null

        /**
         * Set by [MainActivity] to receive Bluetooth media-button presses as
         * `(androidKeyCode, pressed)`.
         */
        @Volatile
        var onMediaButton: ((Int, Boolean) -> Unit)? = null

        @Volatile
        private var instance: OverlayService? = null

        val isRunning: Boolean get() = instance != null

        /** Pushes the current call state onto the overlay. */
        fun updateState(
            names: List<String>,
            transmitting: Boolean,
            connected: Boolean,
            muted: Boolean,
            deafened: Boolean,
            level: Float,
            threshold: Float,
            noiseFloor: Float,
            speaking: Boolean,
        ) {
            instance?.applyState(
                names, transmitting, connected, muted, deafened,
                level, threshold, noiseFloor, speaking,
            )
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        instance = this
        startAsForeground()
        setUpMediaSession()
        addOverlay()
    }

    /**
     * Registers a media session so Bluetooth remotes reach us in the background.
     *
     * A handlebar remote is the practical way to key a microphone with gloves
     * on, and most present as a Bluetooth HID device sending media keys. Those
     * only reach an app that owns an *active* media session with a playback
     * state — a session that reports "stopped" is skipped in favour of whatever
     * music app is actually playing.
     */
    private fun setUpMediaSession() {
        if (mediaSession != null) return
        val session = MediaSession(this, "MumbleWay")

        session.setCallback(object : MediaSession.Callback() {
            override fun onMediaButtonEvent(intent: Intent): Boolean {
                val event: KeyEvent? =
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT, KeyEvent::class.java)
                    } else {
                        @Suppress("DEPRECATION")
                        intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT)
                    }
                if (event == null) return false

                // Ignore auto-repeat: holding a remote button should not fire
                // a toggle over and over.
                if (event.repeatCount > 0) return true

                val pressed = event.action == KeyEvent.ACTION_DOWN
                onMediaButton?.invoke(event.keyCode, pressed)
                return true
            }
        })

        session.setPlaybackState(
            PlaybackState.Builder()
                .setActions(
                    PlaybackState.ACTION_PLAY_PAUSE or
                        PlaybackState.ACTION_PLAY or
                        PlaybackState.ACTION_PAUSE or
                        PlaybackState.ACTION_SKIP_TO_NEXT or
                        PlaybackState.ACTION_SKIP_TO_PREVIOUS or
                        PlaybackState.ACTION_STOP,
                )
                // Claiming "playing" is what puts us at the front of the media
                // button queue. The session carries no audio of its own; voice
                // goes through the engine's own stream.
                .setState(PlaybackState.STATE_PLAYING, 0L, 1.0f)
                .build(),
        )
        session.isActive = true
        mediaSession = session
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        // Restart if the system kills us: losing the talk button mid-ride is
        // exactly the failure this feature exists to prevent.
        return START_STICKY
    }

    override fun onDestroy() {
        removeOverlay()
        mediaSession?.apply {
            isActive = false
            release()
        }
        mediaSession = null
        instance = null
        super.onDestroy()
    }

    private fun startAsForeground() {
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Push-to-talk overlay",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "Keeps the talk button available over other apps"
                setShowBadge(false)
            }
            manager.createNotificationChannel(channel)
        }

        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )

        val notification: Notification = Notification.Builder(this, CHANNEL_ID)
            .setContentTitle("MumbleWay")
            .setContentText("Push-to-talk is available over other apps")
            .setSmallIcon(android.R.drawable.ic_btn_speak_now)
            .setContentIntent(open)
            .setOngoing(true)
            .build()

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            // Android 14 insists a foreground service declares why it exists.
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun dp(value: Int): Int = TypedValue.applyDimension(
        TypedValue.COMPLEX_UNIT_DIP,
        value.toFloat(),
        resources.displayMetrics,
    ).toInt()

    private fun addOverlay() {
        if (root != null) return
        val wm = getSystemService(Context.WINDOW_SERVICE) as WindowManager
        windowManager = wm

        val container = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(10), dp(8), dp(14), dp(8))
            background = GradientDrawable().apply {
                cornerRadius = dp(28).toFloat()
                setColor(Color.argb(235, 20, 22, 26))
                setStroke(dp(1), Color.argb(90, 255, 255, 255))
            }
        }

        val talk = TextView(this).apply {
            text = "TALK"
            setTextColor(Color.WHITE)
            textSize = 13f
            gravity = Gravity.CENTER
            setPadding(dp(14), dp(12), dp(14), dp(12))
            background = idleTalkBackground()
        }

        val names = TextView(this).apply {
            text = "No one speaking"
            setTextColor(Color.argb(200, 255, 255, 255))
            textSize = 12f
            maxLines = 2
            setPadding(dp(10), 0, 0, 0)
            maxWidth = dp(150)
        }

        // Glyphs rather than words: the row has to stay narrow enough to leave
        // the navigation app usable, and these are read at a glance at speed.
        val mute = pill("\u{1F3A4}")
        val deafen = pill("\u{1F50A}")
        val hangup = pill("\u{2715}").apply { setTextColor(Color.argb(255, 255, 138, 128)) }

        val dot = DotView(this)
        val meter = MeterView(this)

        // Names and meter stack vertically so the island stays narrow enough
        // to leave the navigation app usable.
        val label = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(
                dot,
                LinearLayout.LayoutParams(dp(8), dp(8)).apply {
                    rightMargin = dp(6)
                },
            )
            addView(names)
        }

        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(10), 0, dp(4), 0)
            addView(label)
            addView(
                meter,
                LinearLayout.LayoutParams(dp(120), dp(10)).apply {
                    topMargin = dp(4)
                },
            )
        }

        container.addView(talk)
        container.addView(column)
        container.addView(mute)
        container.addView(deafen)
        container.addView(hangup)

        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
        } else {
            @Suppress("DEPRECATION")
            WindowManager.LayoutParams.TYPE_PHONE
        }

        layoutParams = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            type,
            // Not focusable, so the navigation app underneath keeps receiving
            // input everywhere except on this small window.
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            android.graphics.PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
            x = dp(12)
            y = dp(140)
        }

        talk.setOnTouchListener { view, event ->
            when (event.action) {
                MotionEvent.ACTION_DOWN -> {
                    view.background = activeTalkBackground()
                    onTransmit?.invoke(true)
                    true
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    view.background = idleTalkBackground()
                    onTransmit?.invoke(false)
                    true
                }
                else -> false
            }
        }

        mute.setOnClickListener { onToggleMute?.invoke() }
        deafen.setOnClickListener { onToggleDeafen?.invoke() }
        hangup.setOnClickListener { onHangup?.invoke() }

        // Dragging by the label area lets the rider move the island clear of
        // whatever the navigation app is showing.
        names.setOnTouchListener(DragHandler())

        wm.addView(container, layoutParams)
        root = container
        talkButton = talk
        namesView = names
        muteButton = mute
        deafenButton = deafen
        meterView = meter
        dotView = dot
    }

    /**
     * Input level with the noise floor and the activation threshold marked on
     * the same scale.
     *
     * The two markers are separate because the gap between them is the margin
     * being tuned. On a bike the floor climbs with road speed, and a meter
     * showing only the threshold makes that look like a control that has
     * drifted rather than wind.
     */
    private inner class MeterView(context: Context) : View(context) {
        var level = 0f
        var threshold = 0f
        var noiseFloor = 0f

        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        private val rect = RectF()

        override fun onDraw(canvas: Canvas) {
            val h = dp(5).toFloat()
            val top = (height - h) / 2f
            val r = h / 2f

            rect.set(0f, top, width.toFloat(), top + h)
            paint.color = Color.argb(60, 255, 255, 255)
            canvas.drawRoundRect(rect, r, r, paint)

            val filled = level.coerceIn(0f, 1f)
            if (filled > 0.001f) {
                // Colour answers the only question the meter is asked: would
                // this level open the gate?
                paint.color = if (level >= threshold) {
                    Color.argb(255, 92, 217, 115)
                } else {
                    Color.argb(255, 184, 184, 184)
                }
                rect.set(0f, top, (width * filled).coerceAtLeast(h), top + h)
                canvas.drawRoundRect(rect, r, r, paint)
            }

            tick(canvas, noiseFloor, Color.argb(255, 140, 170, 210), dp(2).toFloat(), top, h)
            tick(canvas, threshold, Color.argb(255, 255, 199, 64), dp(4).toFloat(), top, h)
        }

        private fun tick(
            canvas: Canvas,
            value: Float,
            colour: Int,
            overhang: Float,
            top: Float,
            h: Float,
        ) {
            paint.color = colour
            val x = width * value.coerceIn(0f, 1f)
            val w = dp(1).toFloat()
            canvas.drawRect(x - w, top - overhang, x + w, top + h + overhang, paint)
        }
    }

    /**
     * The transmit indicator. Blinks while the microphone is going out, the way
     * a studio on-air light does: a steady colour is easy to stop noticing, and
     * a channel left keyed open is the failure worth catching.
     */
    private inner class DotView(context: Context) : View(context) {
        var colour = Color.GRAY
        var lit = true

        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)

        override fun onDraw(canvas: Canvas) {
            paint.color = colour
            paint.alpha = if (lit) 255 else 64
            val r = minOf(width, height) / 2f
            canvas.drawCircle(width / 2f, height / 2f, r, paint)
        }
    }

    private fun pill(glyph: String) = TextView(this).apply {
        text = glyph
        setTextColor(Color.WHITE)
        textSize = 13f
        gravity = Gravity.CENTER
        setPadding(dp(9), dp(9), dp(9), dp(9))
        background = pillBackground(active = false)
    }

    private fun pillBackground(active: Boolean) = GradientDrawable().apply {
        cornerRadius = dp(18).toFloat()
        setColor(
            if (active) Color.argb(255, 205, 110, 40) else Color.argb(255, 55, 60, 68),
        )
    }

    private fun idleTalkBackground() = GradientDrawable().apply {
        cornerRadius = dp(22).toFloat()
        setColor(Color.argb(255, 55, 60, 68))
    }

    private fun activeTalkBackground() = GradientDrawable().apply {
        cornerRadius = dp(22).toFloat()
        setColor(Color.argb(255, 52, 152, 219))
    }

    private inner class DragHandler : View.OnTouchListener {
        private var startX = 0
        private var startY = 0
        private var touchX = 0f
        private var touchY = 0f
        private var moved = false

        override fun onTouch(view: View, event: MotionEvent): Boolean {
            when (event.action) {
                MotionEvent.ACTION_DOWN -> {
                    startX = layoutParams.x
                    startY = layoutParams.y
                    touchX = event.rawX
                    touchY = event.rawY
                    moved = false
                    return true
                }
                MotionEvent.ACTION_MOVE -> {
                    val dx = (event.rawX - touchX).toInt()
                    val dy = (event.rawY - touchY).toInt()
                    if (abs(dx) > dp(4) || abs(dy) > dp(4)) moved = true
                    layoutParams.x = startX + dx
                    layoutParams.y = startY + dy
                    root?.let { windowManager?.updateViewLayout(it, layoutParams) }
                    return true
                }
                MotionEvent.ACTION_UP -> {
                    if (!moved) view.performClick()
                    return true
                }
            }
            return false
        }
    }

    private fun applyState(
        names: List<String>,
        transmitting: Boolean,
        connected: Boolean,
        muted: Boolean,
        deafened: Boolean,
        level: Float,
        threshold: Float,
        noiseFloor: Float,
        speaking: Boolean,
    ) {
        val view = namesView ?: return
        val talk = talkButton ?: return
        view.post {
            view.text = when {
                !connected -> "Not connected"
                deafened -> "Deafened"
                muted -> "Muted"
                names.isEmpty() -> "No one speaking"
                names.size <= 2 -> names.joinToString(", ")
                else -> "${names.take(2).joinToString(", ")} +${names.size - 2}"
            }
            talk.background =
                if (transmitting) activeTalkBackground() else idleTalkBackground()
            muteButton?.background = pillBackground(active = muted)
            deafenButton?.background = pillBackground(active = deafened)

            meterView?.let {
                it.level = level
                it.threshold = threshold
                it.noiseFloor = noiseFloor
                it.invalidate()
            }

            dotView?.let {
                it.colour = when {
                    !connected -> Color.GRAY
                    transmitting -> Color.argb(255, 240, 62, 62)
                    speaking -> Color.argb(255, 64, 199, 115)
                    else -> Color.GRAY
                }
                it.invalidate()
            }
            setFlashing(transmitting)
        }
    }

    /**
     * Drives the on-air blink from its own runnable rather than from state
     * pushes, so the rate stays even no matter how often there is something
     * new to report.
     */
    private fun setFlashing(on: Boolean) {
        if (on == flashing) return
        flashing = on
        flashHandler.removeCallbacksAndMessages(null)
        if (!on) {
            dotView?.let {
                it.lit = true
                it.invalidate()
            }
            return
        }
        val blink = object : Runnable {
            override fun run() {
                val dot = dotView ?: return
                dot.lit = !dot.lit
                dot.invalidate()
                flashHandler.postDelayed(this, 350)
            }
        }
        flashHandler.postDelayed(blink, 350)
    }

    private fun removeOverlay() {
        val view = root ?: return
        try {
            windowManager?.removeView(view)
        } catch (_: IllegalArgumentException) {
            // Already gone; nothing to undo.
        }
        root = null
        talkButton = null
        namesView = null
        muteButton = null
        deafenButton = null
        meterView = null
        dotView = null
        flashHandler.removeCallbacksAndMessages(null)
        flashing = false
    }
}
