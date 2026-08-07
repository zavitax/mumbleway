package com.mumbleway.mumbleway

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioDeviceInfo
import android.media.AudioManager
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
    fun activate(done: (Boolean) -> Unit) {
        if (active) {
            done(true)
            return
        }
        previousMode = audio.mode
        audio.mode = AudioManager.MODE_IN_COMMUNICATION
        active = true

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            done(chooseCommunicationDevice())
            return
        }
        @Suppress("DEPRECATION")
        run {
            if (!audio.isBluetoothScoAvailableOffCall) {
                // No hands-free route to be had. Not a failure: the phone's own
                // microphone is a perfectly good fallback and is what a rider
                // without a headset is using anyway.
                audio.isSpeakerphoneOn = true
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
            ?: return true

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
