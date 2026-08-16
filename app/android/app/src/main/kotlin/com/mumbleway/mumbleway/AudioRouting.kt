package com.mumbleway.mumbleway

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
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

    /** Held for as long as the route is, and given back with it. */
    private var focusRequest: AudioFocusRequest? = null

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
     * Asks the system to duck music rather than stop it, and keeps holding the
     * microphone when it will not.
     *
     * `AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK` is the whole choice here. Plain
     * `AUDIOFOCUS_GAIN` would tell the other app to stop, which is not what a
     * rider wants from an intercom: they are listening to something and would
     * like to keep listening to it between sentences. This asks for it to be
     * turned down instead, which is what the iOS side gets from `.duckOthers`.
     *
     * **Losing focus does not release the microphone.** That is deliberate and
     * it is the Android half of a fault reported on iOS: something else
     * starting audio must not end up silencing a rider mid-ride, with the only
     * symptom being that nobody answers. A call outlives another app's music.
     * The listener exists so the loss is recorded rather than acted on.
     *
     * Not fatal if refused. Another app can hold focus exclusively, and the
     * right response is a call with music over the top of it, not no call.
     */
    private fun requestMusicFocus() {
        if (focusRequest != null) return
        val request =
            AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build()
                )
                // Nothing here waits for focus, so a delayed grant would arrive
                // after the call it was for.
                .setAcceptsDelayedFocusGain(false)
                .setOnAudioFocusChangeListener { change ->
                    Log.i(TAG, "audio focus changed: $change (the call keeps the microphone)")
                }
                .build()
        focusRequest = request
        val granted = audio.requestAudioFocus(request)
        if (granted != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            Log.i(TAG, "audio focus refused ($granted); carrying on without ducking")
        }
    }

    private fun abandonMusicFocus() {
        val request = focusRequest ?: return
        focusRequest = null
        try {
            audio.abandonAudioFocusRequest(request)
        } catch (e: Exception) {
            Log.w(TAG, "could not give back audio focus", e)
        }
    }

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

    fun activate(done: (Boolean) -> Unit) {
        if (active) {
            done(true)
            return
        }
        previousMode = audio.mode
        // Before the mode change, so whatever is playing is already ducking by
        // the time the route moves under it.
        requestMusicFocus()
        audio.mode = AudioManager.MODE_IN_COMMUNICATION
        active = true
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
     * Hands the route back.
     *
     * Unconditionally, and swallowing failures. A phone left in
     * `MODE_IN_COMMUNICATION` routes every other app's audio to the earpiece
     * and keeps the SCO link alive, which flattens a headset's battery — and
     * the rider has no way to know that is what happened or which app to blame.
     */
    fun deactivate() {
        if (!active) return
        active = false
        stopWatchingForSilencing()
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
        try {
            audio.mode = previousMode
        } catch (e: Exception) {
            Log.w(TAG, "could not restore the audio mode", e)
        }
        // Last, so music comes back up to a phone that has already stopped
        // being a telephone.
        abandonMusicFocus()
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

        routeCode =
            when (target.type) {
                AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> 3
                AudioDeviceInfo.TYPE_BUILTIN_SPEAKER,
                AudioDeviceInfo.TYPE_BUILTIN_MIC -> 1
                AudioDeviceInfo.TYPE_WIRED_HEADSET,
                AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> 2
                AudioDeviceInfo.TYPE_USB_DEVICE,
                AudioDeviceInfo.TYPE_USB_HEADSET -> 4
                else -> 5
            }
        return try {
            audio.setCommunicationDevice(target)
        } catch (e: Exception) {
            Log.w(TAG, "could not select a communication device", e)
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
                } else {
                    Log.w(TAG, "SCO did not come up in time; using the built-in microphone")
                    audio.isSpeakerphoneOn = true
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
