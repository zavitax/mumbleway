package com.mumbleway.mumbleway

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.media.AudioRecordingConfiguration
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log

/**
 * Puts Android into a call, and points it at the helmet's microphone.
 *
 * This did not exist, and its absence was a bug of exactly the same shape as
 * the iOS one it is named after — a Bluetooth headset that plays audio
 * perfectly while the far end hears the phone's own microphone, or nothing.
 *
 * Android will not use a headset's microphone because a headset is connected.
 * A2DP, the profile a paired headset arrives on, is output-only; the
 * microphone lives on the hands-free profile, and that link is not established
 * until an app asks for it by declaring that it is in a call. An app that never
 * asks gets the built-in microphone, which inside a helmet at speed records the
 * inside of a helmet at speed.
 *
 * Nothing fails and nothing logs. The recording indicator lights, the meter
 * moves — because the phone's own microphone is picking up wind through a
 * jacket pocket — and the far end hears roaring. It is the same silent failure
 * as the iOS fault and it needed the same thing: asking for the route rather
 * than hoping for it.
 */
class AudioRouting(private val context: Context) {
    private val audio: AudioManager =
        context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    /** Restored on the way out, so leaving a call does not reshape the phone. */
    private var previousMode: Int = AudioManager.MODE_NORMAL
    private var active = false

    /**
     * Told when another app takes the microphone out from under us.
     *
     * **The failure this reports is otherwise invisible.** From Android 10 two
     * apps may capture at once and only one of them gets real audio; the other
     * is handed digital silence, with no error, no callback of its own and
     * nothing in the stream to distinguish it from a quiet room. A navigation
     * app listening for voice commands is the common case, and the report that
     * reaches us is "nobody can hear me" — which is indistinguishable from a
     * dozen other faults.
     *
     * `isClientSilenced` is the platform saying it in as many words, so it is
     * asked rather than inferred. The capture worker already warns on two
     * seconds of bit-exact zero, which catches the same thing from the other
     * side and cannot say why.
     */
    var onSilenced: ((Boolean) -> Unit)? = null

    private var silenced = false
    private var recordingCallback: AudioManager.AudioRecordingCallback? = null

    /**
     * **There is no audio-focus request in this class, and that is a
     * withdrawal rather than an omission.**
     *
     * It held `AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK` from [activate] to
     * [deactivate] for months, which asks the system to turn other apps down
     * while it is held. A rider reported the consequence: music quieter for
     * the whole ride with nobody talking for any of it. Holding the focus *is*
     * the duck — the three gain types mean stop, pause and duck, and none of
     * them is "I hold focus and you carry on at full volume" — so there was
     * nothing to switch off except the lease itself.
     *
     * It then became per-utterance, taken as people spoke and given back
     * fifteen seconds after the last voice, which works on this platform:
     * focus is orthogonal to the route, so unlike iOS nothing about the input
     * could be disturbed by it. That version was never published. **The iOS
     * half of the same feature stopped a rider's background audio for good**
     * (see `AudioSession.swift`'s `setCapturingCategory`), and with the
     * feature gone there the rider asked for it gone here too, which leaves
     * one behaviour to explain instead of two.
     *
     * The evidence that it was worth having was never there either, in the
     * only form that counts — a rider who heard both said the mix is fine
     * without it: *"we normalize incoming speaker audio, so it mixes nicely
     * with the music"*.
     *
     * So other apps now play at their own volume through a call. One thing
     * this does not control and should not be mistaken for ducking:
     * `MODE_IN_COMMUNICATION` reroutes and on many devices attenuates other
     * apps' media by itself — see [deactivate] on why a phone must not be
     * left in it. `docs/CAPTURE_ON_DEMAND.md` has both halves of the story.
     */

    /**
     * Takes the route for a call, reporting when the microphone is actually
     * reachable.
     *
     * Asynchronous on purpose. On the older path the SCO link is negotiated
     * over the air and takes a second or more, and `startBluetoothSco` returns
     * long before it is up. Returning immediately would have the engine open
     * the device while the route is still the built-in microphone, and it
     * would then keep that microphone for the whole call — the stream is bound
     * at open time and does not follow a route that changes underneath it.
     */
    /**
     * Which microphone we ended up on, as the code the decision log carries.
     *
     * **The numbers belong to `record.rs` and are mirrored here.** They are a
     * wire format: recordings already on riders' phones are read with that
     * table, so a number cannot change meaning. 0 unknown, 1 the phone's own,
     * 2 wired, 3 Bluetooth hands-free, 4 USB, 5 something else.
     *
     * Reported because a recording made through the wrong microphone looks
     * exactly like one made through the right one — the fault the diagnostic
     * recorder exists to prevent, and which it could not answer for the route.
     * A quiet recording arrived and the device had to be inferred from the
     * audio's bandwidth, a hands-free link stopping dead at 3.4 kHz where a
     * built-in microphone runs to 16.
     */
    var routeCode: Int = 0
        private set

    /**
     * Watches for the platform silencing our capture.
     *
     * API 29, which is where concurrent capture and `isClientSilenced` both
     * arrive. Below it a second app simply could not open the microphone, so
     * there is nothing to watch for.
     */
    private fun watchForSilencing() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return
        if (recordingCallback != null) return
        val cb =
            object : AudioManager.AudioRecordingCallback() {
                override fun onRecordingConfigChanged(
                    configs: MutableList<AudioRecordingConfiguration>?
                ) {
                    // Ours are the ones this process owns. `configs` carries
                    // every app's, and another app being silenced is not our
                    // business — it may well be us doing the silencing.
                    val mine =
                        configs?.filter { it.clientAudioSource != AudioManager.ERROR }
                            ?: return
                    val nowSilenced = mine.any { it.isClientSilenced }
                    if (nowSilenced != silenced) {
                        silenced = nowSilenced
                        Log.w(
                            TAG,
                            if (nowSilenced)
                                "another app has taken the microphone; we are being fed silence"
                            else "the microphone is ours again",
                        )
                        onSilenced?.invoke(nowSilenced)
                    }
                }
            }
        recordingCallback = cb
        audio.registerAudioRecordingCallback(cb, Handler(Looper.getMainLooper()))
    }

    private fun stopWatchingForSilencing() {
        val cb = recordingCallback ?: return
        recordingCallback = null
        silenced = false
        try {
            audio.unregisterAudioRecordingCallback(cb)
        } catch (e: Exception) {
            Log.w(TAG, "could not unregister the recording callback", e)
        }
    }

    /**
     * Whether a headset is attached at all, whichever microphone is live.
     *
     * **Not derivable from [routeCode], and that is why this exists.** In
     * pocket mode the communication device is released while only listening,
     * so a rider with a helmet on their head reports the phone's own
     * microphone — the answer would be "no helmet" for the whole of the state
     * the question matters in.
     *
     * The output devices are what know: music is going to the helmet over
     * A2DP whether or not this app holds the hands-free profile. Wired and USB
     * headsets show up in the same list.
     */
    fun headsetPresent(): Boolean {
        val outputs = audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS)
        return outputs.any {
            when (it.type) {
                AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
                AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
                AudioDeviceInfo.TYPE_WIRED_HEADSET,
                AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
                AudioDeviceInfo.TYPE_USB_HEADSET,
                AudioDeviceInfo.TYPE_USB_DEVICE,
                -> true
                else -> false
            }
        }
    }

    /** Whether the hands-free profile is currently held. */
    private var capturing = false

    /**
     * Takes the session, and the profile with it unless capture is on demand.
     *
     * **The split is along session versus profile**, and on Android it falls out
     * almost for free: the saved mode is the session, while
     * `MODE_IN_COMMUNICATION` and the communication device *are* the hands-free
     * profile. See `docs/CAPTURE_ON_DEMAND.md`. Audio focus used to be the
     * other half of "the session"; the comment above `activate` says where it
     * went.
     *
     * **This always takes capture**, and the `captureOnDemand` that stopped at
     * the listening state is gone. It made connecting the moment that decided
     * whether a microphone existed, which is the wrong place for that decision
     * and the wrong time to get it wrong. Pocket mode calls `stopCapture`
     * afterwards, which drops back to `MODE_NORMAL` with no communication
     * device — so the output stream (carrying AAudio's default media usage,
     * because the vendored cpal backend never calls `setUsage`) rides A2DP and
     * music plays at full bandwidth, exactly as the listening branch did.
     */
    fun activate(done: (Boolean) -> Unit) {
        if (!active) {
            previousMode = audio.mode
            active = true
            // Before the watcher, which fires once on registration with the
            // devices that are already there.
            headsetWas = headsetPresent()
            watchForRouteChanges()
        }
        startCapture(done)
    }

    /**
     * Takes the hands-free profile, which is what makes a microphone exist.
     *
     * Asynchronous because an SCO link has to be negotiated, and the caller
     * needs to know *when* — the cue that tells a rider they may speak is only
     * honest if it fires once input is genuinely live.
     */
    fun startCapture(done: (Boolean) -> Unit) {
        if (!active) {
            // Nothing to take a profile on. Refused rather than silently
            // activating: an ordering mistake should be visible once, not
            // produce a session nobody asked for.
            Log.w(TAG, "startCapture with no session")
            done(false)
            return
        }
        if (capturing) {
            done(true)
            return
        }
        capturing = true
        audio.mode = AudioManager.MODE_IN_COMMUNICATION
        watchForSilencing()

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            done(chooseCommunicationDevice())
            return
        }
        routeCode = 0
        @Suppress("DEPRECATION")
        run {
            if (!audio.isBluetoothScoAvailableOffCall) {
                // No hands-free route to be had. Not a failure: the phone's own
                // microphone is a perfectly good fallback and is what a rider
                // without a headset is using anyway.
                audio.isSpeakerphoneOn = true
                routeCode = 1
                done(true)
                return
            }
            awaitSco(done)
        }
    }

    /**
     * Gives the hands-free profile back while keeping the session.
     *
     * The point of the whole exercise: with the mode and the communication
     * device released, the headset falls back to A2DP and everything the rider
     * hears — the group, and whatever music is playing — returns to full
     * bandwidth.
     */
    fun stopCapture() {
        if (!capturing) return
        capturing = false
        stopWatchingForSilencing()
        releaseCommunicationRoute()
        try {
            audio.mode = AudioManager.MODE_NORMAL
        } catch (e: Exception) {
            Log.w(TAG, "could not leave communication mode", e)
        }
        routeCode = currentRouteCode()
    }

    private fun releaseCommunicationRoute() {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                audio.clearCommunicationDevice()
            } else {
                @Suppress("DEPRECATION")
                run {
                    audio.stopBluetoothSco()
                    audio.isBluetoothScoOn = false
                    audio.isSpeakerphoneOn = false
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "could not release the communication route", e)
        }
    }

    /**
     * Hands everything back.
     *
     * Unconditionally, and swallowing failures. A phone left in
     * `MODE_IN_COMMUNICATION` routes every other app's audio to the earpiece
     * and keeps the SCO link alive, which flattens a headset's battery — and
     * the rider has no way to know that is what happened or which app to blame.
     */
    fun deactivate() {
        if (!active) return
        active = false
        capturing = false
        stopWatchingForSilencing()
        stopWatchingForRouteChanges()
        releaseCommunicationRoute()
        try {
            audio.mode = previousMode
        } catch (e: Exception) {
            Log.w(TAG, "could not restore the audio mode", e)
        }
    }

    /**
     * Told when the system moves the microphone to a different device.
     *
     * **A headset reconnecting mid-ride used to go unreported.** `routeCode` was
     * written once during [activate] and never again, so every recording made
     * after a reconnect carried the route the session started on — and the route
     * column exists precisely because the audio itself does not say what
     * captured it.
     *
     * Carries `Recorded::route`'s code. Fired when that code changes **or when
     * [headsetPresent] does**, because the platform reports device arrivals
     * liberally and a notification per Bluetooth housekeeping event is noise —
     * but those are the two facts Dart cannot ask for.
     *
     * **The second condition is the whole of pocket mode on this platform.**
     * While only listening, the communication device is released, so the route
     * is the phone's own microphone *whether or not a headset is attached* —
     * which means a headset arriving or dropping changes the route code not at
     * all, the early return below swallowed it, and Dart never learned. Pocket
     * mode then sat in the wrong half of itself: a headset connected and
     * capture never handed over to listening, or a headset gone and the
     * microphone left closed with nothing able to open it. The iOS side has
     * the same fact reported from a route-change notification it gets for
     * free; this is the Android half of it.
     */
    var onRouteChanged: ((Int) -> Unit)? = null

    /**
     * A line for the engine's log, which is the one a rider can send.
     *
     * **`Log.i` reaches logcat, and logcat needs a cable.** Every Android
     * fault in this feature has been diagnosed by reasoning, because the only
     * account of what the routing actually did was on a developer's machine —
     * while the iOS half has reported its route changes into the engine log
     * since the day three defects in a row came out of them. The log is on
     * screen in the diagnostics panel with a copy button, and it travels with
     * a shared recording.
     */
    var onEvent: ((String) -> Unit)? = null

    private var deviceCallback: android.media.AudioDeviceCallback? = null
    private var commDeviceListener: Any? = null

    /**
     * `Recorded::route`'s table, which is a wire format — see `record.rs`. The
     * numbers must not be renumbered and a new route takes the next free one.
     */
    private fun codeForType(type: Int): Int =
        when (type) {
            AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> 3
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER,
            AudioDeviceInfo.TYPE_BUILTIN_MIC -> 1
            AudioDeviceInfo.TYPE_WIRED_HEADSET,
            AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> 2
            AudioDeviceInfo.TYPE_USB_DEVICE,
            AudioDeviceInfo.TYPE_USB_HEADSET -> 4
            else -> 5
        }

    /**
     * What the route is *now*, read from the platform rather than remembered.
     *
     * Asked rather than inferred for the same reason [usingHeadsetMic] is: a
     * preference stated at activation is not a promise, and the case worth
     * catching is a connected helmet unit on output while the recording comes
     * from a phone in a pocket.
     */
    fun currentRouteCode(): Int {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val type = audio.communicationDevice?.type ?: return 0
            return codeForType(type)
        }
        // Below 31 there is no communication device to ask about. SCO being on
        // is the only signal, and it is the one that matters: it is the
        // narrowband route whose bandwidth a reader would otherwise have to
        // infer from a spectrum.
        @Suppress("DEPRECATION")
        return if (audio.isBluetoothScoOn) 3 else routeCode
    }

    /**
     * The last headset presence reported, so a change in it can be noticed.
     *
     * Seeded in [activate] rather than left false, so the callback Android
     * fires the instant it is registered — with the current device list — does
     * not count as news.
     */
    private var headsetWas = false

    private fun reportRouteIfChanged() {
        if (!active) return
        val now = currentRouteCode()
        val headset = headsetPresent()
        if (now == routeCode && headset == headsetWas) return
        val line = "route changed: $routeCode -> $now, headset $headsetWas -> $headset"
        Log.i(TAG, line)
        onEvent?.invoke(line)
        routeCode = now
        headsetWas = headset
        onRouteChanged?.invoke(now)
    }

    /**
     * Watches for the route moving under us.
     *
     * Two listeners, because neither alone is enough. The communication-device
     * listener is authoritative about what capture is actually on, and exists
     * only from API 31. `AudioDeviceCallback` reaches back to API 23 and fires
     * when a headset appears or disappears — which is the event a rider
     * experiences, and the one that happens while the app is in a pocket.
     */
    private fun watchForRouteChanges() {
        if (deviceCallback != null) return
        val main = Handler(Looper.getMainLooper())

        val cb =
            object : android.media.AudioDeviceCallback() {
                override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>?) {
                    reportRouteIfChanged()
                }

                override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>?) {
                    reportRouteIfChanged()
                }
            }
        deviceCallback = cb
        runCatching { audio.registerAudioDeviceCallback(cb, main) }
            .onFailure { Log.w(TAG, "could not watch for device changes", it) }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            addCommDeviceListener()
        }
    }

    /**
     * Split out, and annotated, for the same reason [chooseCommunicationDevice]
     * is: the listener *type* is API 31, so naming it inside a version check
     * still trips lint. The field is `Any?` so the class is not referenced at
     * all below 31.
     */
    @androidx.annotation.RequiresApi(Build.VERSION_CODES.S)
    private fun addCommDeviceListener() {
        val listener =
            AudioManager.OnCommunicationDeviceChangedListener { reportRouteIfChanged() }
        commDeviceListener = listener
        runCatching {
            audio.addOnCommunicationDeviceChangedListener(
                java.util.concurrent.Executor { it.run() },
                listener,
            )
        }
            .onFailure { Log.w(TAG, "could not watch the communication device", it) }
    }

    @androidx.annotation.RequiresApi(Build.VERSION_CODES.S)
    private fun removeCommDeviceListener() {
        (commDeviceListener as? AudioManager.OnCommunicationDeviceChangedListener)?.let {
            runCatching { audio.removeOnCommunicationDeviceChangedListener(it) }
        }
    }

    private fun stopWatchingForRouteChanges() {
        deviceCallback?.let { cb ->
            runCatching { audio.unregisterAudioDeviceCallback(cb) }
            deviceCallback = null
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            removeCommDeviceListener()
        }
        commDeviceListener = null
    }

    /** Whether a hands-free microphone is what the route currently uses. */
    fun usingHeadsetMic(): Boolean {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            return audio.communicationDevice?.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO
        }
        @Suppress("DEPRECATION")
        return audio.isBluetoothScoOn
    }

    /**
     * The modern path: name the device rather than starting a link and hoping.
     *
     * `setCommunicationDevice` supersedes `startBluetoothSco` on API 31 and
     * handles the negotiation itself, so there is nothing to wait for here.
     */
    @androidx.annotation.RequiresApi(Build.VERSION_CODES.S)
    private fun chooseCommunicationDevice(): Boolean {
        val devices = audio.availableCommunicationDevices
        val headset = devices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO }
        val target =
            headset
            // No headset: the speaker, not the earpiece. `MODE_IN_COMMUNICATION`
            // alone routes to the earpiece, which is inaudible on a bike and
            // sounds broken indoors — the same reason iOS asks for
            // `.defaultToSpeaker`.
            ?: devices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
            ?: run {
                // Nothing to choose from. Left as whatever the platform picks,
                // and recorded as not known rather than guessed at.
                routeCode = 0
                return true
            }

        return try {
            val ok = audio.setCommunicationDevice(target)
            // **Read back rather than assume.** This used to record the route
            // from the device it *asked* for, before knowing whether the
            // request was honoured — so a failed switch still reported the
            // helmet, and pocket mode's "which microphone is recording" line
            // would have named a microphone that was not being used. That is
            // the exact confusion `CLAUDE.md` says costs a whole measurement,
            // because audio carries no record of what captured it.
            routeCode =
                if (ok) currentRouteCode().let { if (it == 0) codeForType(target.type) else it }
                else currentRouteCode()
            ok
        } catch (e: Exception) {
            Log.w(TAG, "could not select a communication device", e)
            routeCode = currentRouteCode()
            false
        }
    }

    /**
     * The pre-31 path: start the link and wait for it to come up.
     *
     * The broadcast is the only signal that the microphone is reachable. A
     * timeout rather than an indefinite wait, because a headset that never
     * completes the handshake must not hold up a call for ever — better a
     * connection made on the phone's own microphone than one that never
     * happens.
     */
    @Suppress("DEPRECATION")
    private fun awaitSco(done: (Boolean) -> Unit) {
        val main = Handler(Looper.getMainLooper())
        var settled = false
        var receiver: BroadcastReceiver? = null

        val finish = { connected: Boolean ->
            if (!settled) {
                settled = true
                receiver?.let { runCatching { context.unregisterReceiver(it) } }
                if (connected) {
                    audio.isBluetoothScoOn = true
                    // **This was missing.** The pre-31 path set `routeCode = 0`
                    // and never revised it, so every recording from an older
                    // device said "not known" even with a headset on the link —
                    // and 3 is the one code a reader most needs, because it is
                    // the narrowband route.
                    routeCode = 3
                } else {
                    Log.w(TAG, "SCO did not come up in time; using the built-in microphone")
                    audio.isSpeakerphoneOn = true
                    routeCode = 1
                }
                done(true)
            }
        }

        receiver =
            object : BroadcastReceiver() {
                override fun onReceive(c: Context?, intent: Intent?) {
                    val state =
                        intent?.getIntExtra(
                            AudioManager.EXTRA_SCO_AUDIO_STATE,
                            AudioManager.SCO_AUDIO_STATE_ERROR,
                        )
                    when (state) {
                        AudioManager.SCO_AUDIO_STATE_CONNECTED -> finish(true)
                        AudioManager.SCO_AUDIO_STATE_ERROR,
                        AudioManager.SCO_AUDIO_STATE_DISCONNECTED -> finish(false)
                    }
                }
            }

        val filter = IntentFilter(AudioManager.ACTION_SCO_AUDIO_STATE_UPDATED)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            context.registerReceiver(receiver, filter)
        }

        try {
            audio.startBluetoothSco()
        } catch (e: Exception) {
            Log.w(TAG, "could not start SCO", e)
            finish(false)
            return
        }
        main.postDelayed({ finish(false) }, SCO_TIMEOUT_MS)
    }

    private companion object {
        const val TAG = "MumbleWayAudio"

        /**
         * How long to wait for the hands-free link.
         *
         * Generous. A headset waking from idle can take a second and a half,
         * and this is spent once while a connection is being set up, not
         * between words.
         */
        const val SCO_TIMEOUT_MS = 4_000L
    }
}
