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
import android.graphics.Typeface
import kotlin.math.abs

/** Somebody currently being heard, with their level on the shared 0..1 scale. */
data class OverlaySpeaker(val name: String, val level: Float)

/**
 * Everything the window draws, in one piece.
 *
 * A single immutable snapshot rather than a dozen setters, because the frame is
 * redrawn whole: half-applied state would show a rider "talking" beside "not
 * connected", and the pair is worse than either.
 */
data class OverlayState(
    val speakers: List<OverlaySpeaker> = emptyList(),
    /** 0 push to talk, 1 voice activated, 2 always on. */
    val micMode: Int = 0,
    /** Whether audio is actually going out, by whatever route. */
    val live: Boolean = false,
    /** Built in Dart, where the counts and the grammar live together. */
    val connectionText: String = "",
    /** 0 idle, 1 well, 2 struggling, 3 lost. Colour only; no words. */
    val connectionLevel: Int = 0,
    val moreSpeakers: String = "",
    val transmitting: Boolean = false,
    val connected: Boolean = false,
    val muted: Boolean = false,
    val deafened: Boolean = false,
    val level: Float = 0f,
    val threshold: Float = 0f,
    val noiseFloor: Float = 0f,
    val speaking: Boolean = false,
)

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
    private var callView: CallView? = null
    private var talkButton: TextView? = null
    private var muteButton: TextView? = null
    private var deafenButton: TextView? = null
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

        /**
         * The window's wording, in the app's language.
         *
         * Held on the companion rather than the instance so the language
         * survives the service being stopped and started, which happens every
         * time the setting is toggled. Without that the window came back in
         * English until the next time Dart happened to send them.
         */
        @Volatile
        private var phrases: Map<String, String> = emptyMap()

        fun setPhrases(map: Map<String, String>) {
            phrases = map
            instance?.callView?.postInvalidate()
        }

        fun phrase(key: String, fallback: String): String =
            phrases[key]?.takeIf { it.isNotEmpty() } ?: fallback

        /** Pushes the current call state onto the overlay. */
        fun updateState(state: OverlayState) {
            instance?.applyState(state)
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

        // Laid out like the iOS Picture in Picture frame, because it answers
        // the same two questions and a rider who uses both phones should not
        // have to learn the window twice: what this phone is doing with the
        // microphone on the left, who else is talking on the right.
        //
        // The controls sit under the card rather than beside it. iOS gets three
        // system buttons and no choice about them; Android can offer the real
        // set, and putting them on their own row keeps the card's proportions
        // the same as the iOS one rather than squeezing it sideways.
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(6), dp(6), dp(6), dp(6))
            background = GradientDrawable().apply {
                cornerRadius = dp(20).toFloat()
                setColor(Color.argb(235, 16, 24, 34))
                setStroke(dp(1), Color.argb(90, 255, 255, 255))
            }
        }

        val card = CallView(this)

        val talk = TextView(this).apply {
            text = phrase("pipTalk", "TALK")
            setTextColor(Color.WHITE)
            textSize = 13f
            gravity = Gravity.CENTER
            setPadding(dp(14), dp(11), dp(14), dp(11))
            background = idleTalkBackground()
        }

        // Glyphs rather than words: the row has to stay narrow enough to leave
        // the navigation app usable, and these are read at a glance at speed.
        // Kotlin string escapes are UTF-16 and take exactly four hex digits, so
        // anything outside the basic plane is written as a surrogate pair
        // rather than with Swift's \u{...} form.
        val mute = pill("\uD83C\uDFA4")
        val deafen = pill("\uD83D\uDD0A")
        val hangup = pill("\u2715").apply {
            setTextColor(Color.argb(255, 255, 138, 128))
        }

        val controls = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(4), dp(6), dp(4), dp(2))
            addView(
                talk,
                LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f),
            )
            addView(
                mute,
                LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                ).apply { leftMargin = dp(6) },
            )
            addView(
                deafen,
                LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                ).apply { leftMargin = dp(6) },
            )
            addView(
                hangup,
                LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                ).apply { leftMargin = dp(6) },
            )
        }

        container.addView(card, LinearLayout.LayoutParams(dp(300), dp(150)))
        container.addView(controls)

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

        // Dragging by the card lets the rider move the island clear of whatever
        // the navigation app is showing. The card carries no controls of its
        // own, so nothing is lost by making the whole of it the handle.
        card.setOnTouchListener(DragHandler())

        wm.addView(container, layoutParams)
        root = container
        callView = card
        talkButton = talk
        muteButton = mute
        deafenButton = deafen
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
    /**
     * The card, drawn as one piece.
     *
     * A canvas rather than a tree of widgets, for the same reason the iOS frame
     * is one: every element is positioned relative to the others, and expressing
     * that as nested layouts with margins makes a change to one of them move
     * three. The two sides are also meant to stay proportional to each other as
     * the card is resized, which weights and gravities do badly.
     */
    private inner class CallView(context: Context) : View(context) {
        var state = OverlayState()

        /** Blink phase for the on-air light; driven by [setFlashing]. */
        var lit = true

        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        private val text = Paint(Paint.ANTI_ALIAS_FLAG or Paint.SUBPIXEL_TEXT_FLAG)
        private val rect = RectF()

        private fun typeface(bold: Boolean) =
            if (bold) Typeface.DEFAULT_BOLD else Typeface.DEFAULT

        /** Draws one line, returning nothing: every caller places by geometry. */
        private fun label(
            canvas: Canvas,
            value: String,
            x: Float,
            y: Float,
            sizeDp: Int,
            colour: Int,
            bold: Boolean = false,
            align: Paint.Align = Paint.Align.LEFT,
        ) {
            text.textSize = dp(sizeDp).toFloat()
            text.color = colour
            text.typeface = typeface(bold)
            text.textAlign = align
            canvas.drawText(value, x, y, text)
        }

        override fun onDraw(canvas: Canvas) {
            val w = width.toFloat()
            val h = height.toFloat()

            // Two halves, because the window answers two questions that have
            // nothing to do with each other. Stacked they read as one list and
            // the eye has to work out which line belongs to which; side by side
            // each half is glanced at rather than read, which is all a rider
            // has time for.
            val divider = (w * 0.52f)

            paint.color = Color.argb(26, 255, 255, 255)
            canvas.drawRect(divider, dp(14).toFloat(), divider + dp(1), h - dp(14), paint)

            drawConnection(canvas, divider)
            drawOnAir(canvas, divider)
            drawTitle(canvas, divider)
            drawMeter(canvas, divider, h)
            drawSpeakers(canvas, divider, w)
        }

        /**
         * One line across the top saying whether the radio is up.
         *
         * Measured and then centred as one piece rather than each part placed
         * against an edge: the phrase changes length as servers come and go, and
         * anchoring the dot would leave the line shuffling sideways every time.
         */
        private fun drawConnection(canvas: Canvas, divider: Float) {
            val value = state.connectionText.ifEmpty {
                phrase("pipNotConnected", "Not connected")
            }
            paint.color = when (state.connectionLevel) {
                1 -> Color.argb(255, 92, 217, 115)
                2 -> Color.argb(255, 250, 191, 64)
                3 -> Color.argb(255, 240, 82, 71)
                else -> Color.argb(255, 128, 128, 128)
            }

            text.textSize = dp(11).toFloat()
            text.typeface = typeface(true)
            val textWidth = text.measureText(value)
            val diameter = dp(8).toFloat()
            val gap = dp(6).toFloat()
            val startX = divider / 2f - (diameter + gap + textWidth) / 2f
            val middle = dp(18).toFloat()

            canvas.drawCircle(startX + diameter / 2f, middle, diameter / 2f, paint)
            // Centred on the dot rather than sharing its top edge, so the two
            // read as one line.
            label(
                canvas, value, startX + diameter + gap,
                middle - (text.ascent() + text.descent()) / 2f,
                11, Color.WHITE, bold = true,
            )
        }

        private fun drawOnAir(canvas: Canvas, divider: Float) {
            val cx = divider / 2f
            val cy = dp(56).toFloat()
            val radius = dp(20).toFloat()

            val colour = when {
                !state.connected -> Color.argb(255, 115, 115, 115)
                state.live -> Color.argb(255, 240, 62, 62)
                state.speaking -> Color.argb(255, 64, 199, 115)
                else -> Color.argb(255, 140, 140, 140)
            }
            // Steady unless transmitting: a light that is always on is easy to
            // stop noticing, and a channel left keyed open is the failure worth
            // catching.
            val on = !state.live || lit

            if (state.live) {
                paint.color = colour
                paint.alpha = if (on) 77 else 26
                canvas.drawCircle(cx, cy, radius + dp(9), paint)
            }
            paint.color = colour
            paint.alpha = if (on) 255 else 89
            canvas.drawCircle(cx, cy, radius, paint)
            paint.alpha = 255

            if (state.live) {
                label(
                    canvas, phrase("pipOnAir", "ON AIR"), cx,
                    cy - (text.ascent() + text.descent()) / 2f,
                    10, Color.WHITE, bold = true, align = Paint.Align.CENTER,
                )
            } else {
                label(
                    canvas, "🎤", cx, cy - (text.ascent() + text.descent()) / 2f,
                    15, Color.WHITE, align = Paint.Align.CENTER,
                )
            }
        }

        private fun drawTitle(canvas: Canvas, divider: Float) {
            val value = when {
                !state.connected -> phrase("pipNotConnected", "Not connected")
                state.live -> phrase("pipTalking", "Talking")
                state.deafened -> phrase("pipDeafened", "Deafened")
                state.muted -> phrase("pipMuted", "Muted")
                // Not just "Listening": that reads as though the microphone is
                // open, and the point of this line is to say that it is not.
                else -> phrase("pipListening", "Listening, but\nnot transmitting")
            }

            var y = dp(93).toFloat()
            for (line in value.split("\n")) {
                label(
                    canvas, line, divider / 2f, y, 12, Color.WHITE,
                    bold = true, align = Paint.Align.CENTER,
                )
                y += dp(14)
            }

            val badges = buildList {
                if (state.muted) add(phrase("pipBadgeMuted", "MUTED"))
                if (state.deafened) add(phrase("pipBadgeDeafened", "DEAFENED"))
            }
            if (badges.isNotEmpty()) {
                label(
                    canvas, badges.joinToString("  ·  "), divider / 2f, y + dp(2),
                    10, Color.argb(255, 250, 184, 89),
                    bold = true, align = Paint.Align.CENTER,
                )
            }
        }

        /**
         * Input level with the noise floor and the activation threshold marked
         * on the same scale.
         *
         * The two markers are separate because the gap between them is the
         * margin being tuned. On a bike the floor climbs with road speed, and a
         * meter showing only the threshold makes that look like a control that
         * has drifted rather than wind.
         */
        private fun drawMeter(canvas: Canvas, divider: Float, h: Float) {
            val left = dp(16).toFloat()
            val right = divider - dp(16)
            val barHeight = dp(5).toFloat()
            val top = h - dp(18)
            val r = barHeight / 2f

            rect.set(left, top, right, top + barHeight)
            paint.color = Color.argb(60, 255, 255, 255)
            canvas.drawRoundRect(rect, r, r, paint)

            val span = right - left
            val filled = state.level.coerceIn(0f, 1f)
            if (filled > 0.001f) {
                // Colour answers the only question the meter is asked: would
                // this level open the gate?
                paint.color = if (state.level >= state.threshold) {
                    Color.argb(255, 92, 217, 115)
                } else {
                    Color.argb(255, 184, 184, 184)
                }
                rect.set(left, top, left + (span * filled).coerceAtLeast(barHeight), top + barHeight)
                canvas.drawRoundRect(rect, r, r, paint)
            }

            tick(canvas, left, span, state.noiseFloor, Color.argb(255, 140, 170, 210), dp(2).toFloat(), top, barHeight)
            tick(canvas, left, span, state.threshold, Color.argb(255, 255, 199, 64), dp(4).toFloat(), top, barHeight)
        }

        private fun tick(
            canvas: Canvas,
            left: Float,
            span: Float,
            value: Float,
            colour: Int,
            overhang: Float,
            top: Float,
            h: Float,
        ) {
            paint.color = colour
            val x = left + span * value.coerceIn(0f, 1f)
            val w = dp(1).toFloat()
            canvas.drawRect(x - w, top - overhang, x + w, top + h + overhang, paint)
        }

        /**
         * Who is being heard, name against the left edge and level against the
         * right, on one line and centred on each other.
         *
         * A meter under its name reads as a second row and costs the height of
         * one; on the same line the pairing is obvious and the list can breathe.
         * A quarter of the width is enough for a level to be seen moving without
         * taking room from the names, which are what identify who is talking.
         */
        private fun drawSpeakers(canvas: Canvas, divider: Float, w: Float) {
            val left = divider + dp(12)
            val right = w - dp(12)

            if (state.speakers.isEmpty()) {
                label(
                    canvas,
                    if (state.connected) phrase("pipNobodySpeaks", "Nobody speaks") else "—",
                    (left + right) / 2f, dp(78).toFloat(), 12,
                    Color.argb(115, 255, 255, 255), align = Paint.Align.CENTER,
                )
                return
            }

            label(
                canvas, phrase("pipSpeaking", "SPEAKING"), left, dp(20).toFloat(),
                9, Color.argb(102, 255, 255, 255), bold = true,
            )

            val visible = state.speakers.take(4)
            val meterWidth = (right - left) * 0.25f
            val gap = dp(8).toFloat()
            val nameWidth = right - left - meterWidth - gap

            var y = dp(42).toFloat()
            for (speaker in visible) {
                text.textSize = dp(12).toFloat()
                text.typeface = typeface(true)
                label(
                    canvas, ellipsise(speaker.name, nameWidth), left, y, 12,
                    Color.argb(255, 140, 212, 255), bold = true,
                )

                val barHeight = dp(6).toFloat()
                val centre = y - (text.ascent() + text.descent()) / 2f - dp(4)
                val trackTop = centre - barHeight / 2f
                val r = barHeight / 2f
                rect.set(right - meterWidth, trackTop, right, trackTop + barHeight)
                paint.color = Color.argb(31, 255, 255, 255)
                canvas.drawRoundRect(rect, r, r, paint)

                val level = speaker.level.coerceIn(0f, 1f)
                if (level > 0.001f) {
                    // The same three-colour scale as every meter in the app.
                    paint.color = when {
                        level > 0.85f -> Color.argb(255, 240, 82, 71)
                        level > 0.65f -> Color.argb(255, 250, 191, 64)
                        else -> Color.argb(255, 92, 217, 115)
                    }
                    rect.set(
                        right - meterWidth, trackTop,
                        right - meterWidth + (meterWidth * level).coerceAtLeast(barHeight),
                        trackTop + barHeight,
                    )
                    canvas.drawRoundRect(rect, r, r, paint)
                }
                y += dp(24)
            }

            if (state.speakers.size > visible.size && state.moreSpeakers.isNotEmpty()) {
                label(
                    canvas, state.moreSpeakers, left, y, 10,
                    Color.argb(115, 255, 255, 255),
                )
            }
        }

        /** Trims a name to the room it has, rather than letting it run under
         *  the meter. */
        private fun ellipsise(value: String, maxWidth: Float): String {
            if (text.measureText(value) <= maxWidth) return value
            var cut = value.length
            while (cut > 1 && text.measureText(value.take(cut) + "…") > maxWidth) {
                cut--
            }
            return value.take(cut) + "…"
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

    private fun applyState(state: OverlayState) {
        val card = callView ?: return
        val talk = talkButton ?: return
        card.post {
            card.state = state
            card.invalidate()

            // The talk button follows whether audio is actually going out, not
            // whether a finger is down: in the hands-free modes nobody holds
            // anything and the microphone still opens, and a button that stayed
            // grey through all of it would be telling the rider they were off
            // air while they were being heard.
            talk.background =
                if (state.live) activeTalkBackground() else idleTalkBackground()

            // Nothing to hold in the hands-free modes, so the button says so
            // rather than sitting there looking pressable and doing nothing.
            val handsFree = state.micMode != 0
            talk.isEnabled = !handsFree
            talk.alpha = if (handsFree) 0.45f else 1f
            talk.text = when {
                !handsFree -> phrase("pipTalk", "TALK")
                state.micMode == 1 -> phrase("pipHandsFreeVoice", "VOICE")
                else -> phrase("pipHandsFreeAlways", "OPEN")
            }

            muteButton?.background = pillBackground(active = state.muted)
            deafenButton?.background = pillBackground(active = state.deafened)

            setFlashing(state.live)
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
            callView?.let {
                it.lit = true
                it.invalidate()
            }
            return
        }
        val blink = object : Runnable {
            override fun run() {
                val card = callView ?: return
                card.lit = !card.lit
                card.invalidate()
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
        callView = null
        talkButton = null
        muteButton = null
        deafenButton = null
        flashHandler.removeCallbacksAndMessages(null)
        flashing = false
    }
}
