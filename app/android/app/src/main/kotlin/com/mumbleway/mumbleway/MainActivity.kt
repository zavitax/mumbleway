package com.mumbleway.mumbleway

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/**
 * Hosts the Flutter engine and bridges the floating overlay.
 *
 * The overlay lives in a foreground service rather than here so that it
 * survives the activity being backgrounded, which is the normal case: the rider
 * is looking at a navigation app, not at us.
 */
class MainActivity : FlutterActivity() {

    private var channel: MethodChannel? = null
    private var buttonChannel: MethodChannel? = null
    private var logChannel: MethodChannel? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

        // Before anything can start the audio engine.
        //
        // cpal reaches Android's audio APIs through ndk_context, which is a
        // JVM pointer and an application Context that something is expected to
        // have registered. In an ndk-glue app the generated main does it; a
        // Flutter app has no such main, so nothing did, and playing a stream
        // panicked with "android context was not initialized" — which reached
        // the user as "audio device did not start" ten seconds later, from a
        // different thread, with the real message written to a stderr that
        // Android discards.
        nativeSetAndroidContext(applicationContext)

        // Attempted here for the case where the permission is already granted
        // from a previous run; otherwise it starts as soon as it is granted.
        startKeepAliveService()

        // Brings the app forward when the floating window is tapped.
        OverlayService.onOpenApp = {
            runOnUiThread {
                // No NEW_TASK. The manifest gives this activity an empty
                // taskAffinity, so NEW_TASK belongs to no existing task and
                // Android obliges by making another one — which is how tapping
                // the window produced a second running copy of the app. Started
                // from the activity's own context, none is needed: REORDER and
                // SINGLE_TOP bring the instance that already exists forward.
                startActivity(
                    Intent(this, MainActivity::class.java).apply {
                        addFlags(
                            Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or
                                Intent.FLAG_ACTIVITY_SINGLE_TOP,
                        )
                    },
                )
            }
        }

        // The microphone, which nothing had ever asked for.
        //
        // Android grants no dangerous permission from the manifest alone, so
        // recording returned silence on every device: streams opened, the
        // meter sat at zero, and nothing anywhere said why. Answered on the
        // same channel iOS uses, so the Dart side keeps one code path.
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            "mumbleway/audioSession",
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "prepare" -> requestMicrophone(result)
                // Android has no session to take live: the permission is the
                // whole of it, and the engine opens the device itself.
                // Answered rather than left unimplemented so that the Dart side
                // has one code path and iOS is not a special case at the call
                // site as well as behind it.
                "activate" -> result.success(
                    mapOf("ok" to hasMicrophone(), "inputChannels" to -1, "sampleRate" to 0.0)
                )
                "deactivate" -> result.success(true)
                else -> result.notImplemented()
            }
        }

        // Whether there is a conversation to keep the processor awake for.
        //
        // Its own channel rather than the overlay's, even though the wake lock
        // happens to live in the same service that draws the window: a rider
        // who has turned the floating island off in settings still makes
        // calls, and the two ideas only share a service by accident of where
        // Android puts a foreground service.
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            "mumbleway/power",
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "callActive" -> {
                    setCallActive(call.arguments == true)
                    result.success(true)
                }
                else -> result.notImplemented()
            }
        }

        // The engine's own log, repeated into logcat so a device on a cable can
        // be watched live rather than only questioned through the app's panel
        // afterwards. Same lines either way; this copy is the one reachable
        // while the app is misbehaving.
        val logs = MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            "mumbleway/log",
        )
        logChannel = logs
        logs.setMethodCallHandler { call, result ->
            when (call.method) {
                "write" -> {
                    val lines = call.argument<List<Map<String, Any?>>>("lines")
                    if (lines == null) {
                        result.error("args", "write wants a list of lines.", null)
                    } else {
                        for (line in lines) {
                            val message = line["message"] as? String ?: ""
                            when (line["level"] as? Int ?: 2) {
                                0, 1 -> android.util.Log.d(LOG_TAG, message)
                                3 -> android.util.Log.w(LOG_TAG, message)
                                4 -> android.util.Log.e(LOG_TAG, message)
                                else -> android.util.Log.i(LOG_TAG, message)
                            }
                        }
                        result.success(null)
                    }
                }

                else -> result.notImplemented()
            }
        }

        val ch = MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            "mumbleway/overlay",
        )
        channel = ch

        // Button presses on the overlay come back into Dart, which owns the
        // transmit state; one source of truth keeps the overlay and the in-app
        // button from disagreeing.
        OverlayService.onTransmit = { pressed ->
            runOnUiThread { ch.invokeMethod("setTransmitting", pressed) }
        }
        OverlayService.onToggleMute = {
            runOnUiThread { ch.invokeMethod("toggleMute", null) }
        }
        OverlayService.onToggleDeafen = {
            runOnUiThread { ch.invokeMethod("toggleDeafen", null) }
        }

        // Bluetooth media buttons captured by the service's media session.
        // Dart owns the key-to-action binding, so this only reports what was
        // pressed rather than deciding what it means.
        val buttons = MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            "mumbleway/buttons",
        )
        buttonChannel = buttons
        OverlayService.onMediaButton = { keyCode, pressed ->
            runOnUiThread {
                buttons.invokeMethod(
                    "mediaButton",
                    mapOf("keyCode" to keyCode, "pressed" to pressed),
                )
            }
        }

        ch.setMethodCallHandler { call, result ->
            when (call.method) {
                "isSupported" -> result.success(true)

                "hasPermission" -> result.success(canDrawOverlays())

                "requestPermission" -> {
                    if (!canDrawOverlays()) {
                        // Android only allows this to be granted in Settings;
                        // there is no in-app prompt for it.
                        startActivity(
                            Intent(
                                Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                Uri.parse("package:$packageName"),
                            )
                        )
                    }
                    result.success(canDrawOverlays())
                }

                "show" -> {
                    if (!canDrawOverlays()) {
                        result.error(
                            "permission",
                            "Display over other apps is not granted",
                            null,
                        )
                    } else {
                        overlayWanted = true
                        // Only actually put it on screen once the app is behind
                        // something else; see onResume and onPause.
                        if (!inForeground) showOverlayWindow()
                        result.success(true)
                    }
                }

                "hide" -> {
                    overlayWanted = false
                    // The window goes, the service stays: it is what keeps the
                    // call alive in the background, which the rider still wants.
                    hideOverlayWindow()
                    result.success(true)
                }

                "isShowing" -> result.success(OverlayService.isRunning)

                // The window's wording, so it is not stuck in English. Sent
                // separately from the state because it changes when the
                // language does, which is roughly never, while the state
                // changes ten times a second.
                "phrases" -> {
                    @Suppress("UNCHECKED_CAST")
                    val map = call.arguments as? Map<String, String> ?: emptyMap()
                    OverlayService.setPhrases(map)
                    result.success(null)
                }

                "update" -> {
                    // Speakers arrive with their levels already on the app's
                    // shared 0..1 scale, so the window cannot draw a different
                    // loudness from the one on the main screen.
                    val speakers = (call.argument<List<Map<String, Any?>>>("speakers")
                        ?: emptyList()).map {
                        OverlaySpeaker(
                            name = it["name"] as? String ?: "",
                            level = ((it["level"] as? Double) ?: 0.0).toFloat(),
                        )
                    }
                    OverlayService.updateState(
                        OverlayState(
                            speakers = speakers,
                            micMode = call.argument<Int>("micMode") ?: 0,
                            live = call.argument<Boolean>("live") ?: false,
                            connectionText = call.argument<String>("connectionText") ?: "",
                            connectionLevel = call.argument<Int>("connectionLevel") ?: 0,
                            moreSpeakers = call.argument<String>("moreSpeakers") ?: "",
                            othersOnline = call.argument<String>("othersOnline") ?: "",
                            transmitting = call.argument<Boolean>("transmitting") ?: false,
                            connected = call.argument<Boolean>("connected") ?: false,
                            muted = call.argument<Boolean>("muted") ?: false,
                            deafened = call.argument<Boolean>("deafened") ?: false,
                            level = (call.argument<Double>("level") ?: 0.0).toFloat(),
                            threshold = (call.argument<Double>("threshold") ?: 0.0).toFloat(),
                            noiseFloor = (call.argument<Double>("noiseFloor") ?: 0.0).toFloat(),
                            speaking = call.argument<Boolean>("speaking") ?: false,
                        ),
                    )
                    result.success(null)
                }

                else -> result.notImplemented()
            }
        }
    }

    private fun canDrawOverlays(): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            Settings.canDrawOverlays(this)
        } else {
            true
        }

    override fun onDestroy() {
        // The other way out: back-pressed to the end, or finish() called. The
        // recents swipe arrives at the service as onTaskRemoved instead, so
        // both routes need saying.
        if (isFinishing) {
            stopService(Intent(this, OverlayService::class.java))
        }
        OverlayService.onTransmit = null
        OverlayService.onMediaButton = null
        OverlayService.onToggleMute = null
        OverlayService.onToggleDeafen = null
        OverlayService.onOpenApp = null
        channel?.setMethodCallHandler(null)
        buttonChannel?.setMethodCallHandler(null)
        logChannel?.setMethodCallHandler(null)
        super.onDestroy()
    }

    /** Hands the app Context to the Rust audio backend. See the call site. */
    private external fun nativeSetAndroidContext(context: android.content.Context)

    /** Held while the system dialog is up, so the reply reaches the caller. */
    private var micPermissionResult: MethodChannel.Result? = null

    /**
     * Asks for the microphone, answering in the shape the iOS side uses.
     *
     * `inputChannels` is -1 for "not asked", which is what tells the Dart side
     * that a granted permission is enough and it should not go looking for a
     * channel count this platform does not report here.
     */
    private fun requestMicrophone(result: MethodChannel.Result) {
        if (hasMicrophone()) {
            startKeepAliveService()
            result.success(mapOf("granted" to true, "inputChannels" to -1, "sampleRate" to 0.0))
            return
        }
        micPermissionResult = result
        requestPermissions(arrayOf(android.Manifest.permission.RECORD_AUDIO), MIC_REQUEST)
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != MIC_REQUEST) return
        val ok = grantResults.isNotEmpty() &&
            grantResults[0] == android.content.pm.PackageManager.PERMISSION_GRANTED
        // Now that it is allowed, the service that keeps the call alive in the
        // background may legally start.
        if (ok) startKeepAliveService()
        micPermissionResult?.success(
            mapOf("granted" to ok, "inputChannels" to if (ok) -1 else 0, "sampleRate" to 0.0),
        )
        micPermissionResult = null
    }

    /** Whether the user has the floating window switched on. */
    private var overlayWanted = false

    /**
     * Starts true, because the activity is on its way to the front by the time
     * anything can ask.
     *
     * It started false, and Dart switches the window on during startup — before
     * onResume has run — so the first thing on screen was the floating window,
     * with the app behind it and no way through.
     */
    private var inForeground = true

    private fun serviceIntent(action: String) =
        Intent(this, OverlayService::class.java).setAction(action)

    /**
     * Keeps the process alive whether or not a window is wanted.
     *
     * The service and the window are separate concerns. Android suspends a
     * process with nothing in the foreground, which stops the audio threads and
     * drops the call; the service is what prevents that, and it has to run even
     * for a rider who never turns the window on.
     */
    private fun startKeepAliveService() {
        // Not before the microphone is granted. The service declares itself as
        // type "microphone", and from Android 14 starting one of those without
        // RECORD_AUDIO is a SecurityException, not a refusal — so on a fresh
        // install, where the permission has never been asked for, this took the
        // whole app down with "MumbleWay keeps stopping".
        if (!hasMicrophone()) return
        startForegroundService(serviceIntent(OverlayService.ACTION_START))
    }

    private fun hasMicrophone() =
        checkSelfPermission(android.Manifest.permission.RECORD_AUDIO) ==
            android.content.pm.PackageManager.PERMISSION_GRANTED

    private fun showOverlayWindow() {
        if (!canDrawOverlays() || !hasMicrophone()) return
        startForegroundService(serviceIntent(OverlayService.ACTION_SHOW_WINDOW))
    }

    private fun hideOverlayWindow() {
        if (!OverlayService.isRunning) return
        startForegroundService(serviceIntent(OverlayService.ACTION_HIDE_WINDOW))
    }

    /**
     * Tells the service whether a call is up, so it can hold the CPU awake for
     * one and let the phone sleep the rest of the time.
     *
     * Silently ignored when the service is not running, which is the case
     * before the microphone has been granted. There is no call to protect then
     * either, so there is nothing to report.
     */
    private fun setCallActive(active: Boolean) {
        if (!OverlayService.isRunning) return
        startForegroundService(
            serviceIntent(OverlayService.ACTION_SET_CALL_ACTIVE)
                .putExtra(OverlayService.EXTRA_CALL_ACTIVE, active)
        )
    }

    /**
     * The window exists to keep the call reachable while another app is in
     * front, so it has no business being on screen while this one is.
     *
     * Drawing over our own UI is worse than useless: it hides the controls it
     * duplicates, and it cannot be dismissed from the app it is covering. iOS
     * gets this from Picture in Picture, which the system closes on return;
     * Android has to be told, and the activity lifecycle is what knows.
     */
    /**
     * Tied to onStart/onStop rather than onResume/onPause, because "the app is
     * not on screen" is what this is about and only onStop means that.
     *
     * onPause fires whenever the activity merely stops being the top one: a
     * permission dialog, the voice assistant, a notification shade pulled down,
     * the volume panel. All of those leave the app plainly visible underneath,
     * and a floating copy of its own controls would appear over it every time —
     * which is the same "covering our own UI" problem in a form that flickers.
     * onStop is the callback that means the activity is genuinely hidden.
     */
    override fun onStart() {
        super.onStart()
        inForeground = true
        hideOverlayWindow()
    }

    override fun onStop() {
        super.onStop()
        inForeground = false
        // Not on the way out. onStop runs for a close as well as for a
        // backgrounding, and putting the window up as the app is being
        // dismissed is what left it on screen with nothing behind it.
        if (isFinishing) return
        if (overlayWanted) showOverlayWindow()
    }

    private companion object {
        /// Short, because logcat truncates a tag past 23 characters on older
        /// releases and a truncated tag cannot be filtered on.
        const val LOG_TAG = "MumbleWay"
        const val MIC_REQUEST = 4712

        init {
            // Loading it here rather than leaving it to the first Dart call
            // guarantees JNI_OnLoad has run — and so that the JVM pointer
            // exists — before the Context is handed over.
            System.loadLibrary("rust_lib_mumbleway")
        }
    }
}
