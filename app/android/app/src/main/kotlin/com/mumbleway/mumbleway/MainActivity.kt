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
        OverlayService.onHangup = {
            runOnUiThread { ch.invokeMethod("hangup", null) }
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
                        startForegroundService(
                            Intent(this, OverlayService::class.java)
                                .setAction(OverlayService.ACTION_START)
                        )
                        result.success(true)
                    }
                }

                "hide" -> {
                    stopService(Intent(this, OverlayService::class.java))
                    result.success(true)
                }

                "isShowing" -> result.success(OverlayService.isRunning)

                "update" -> {
                    val names = call.argument<List<String>>("names") ?: emptyList()
                    val transmitting = call.argument<Boolean>("transmitting") ?: false
                    val connected = call.argument<Boolean>("connected") ?: false
                    val muted = call.argument<Boolean>("muted") ?: false
                    val deafened = call.argument<Boolean>("deafened") ?: false
                    val level = call.argument<Double>("level") ?: 0.0
                    val threshold = call.argument<Double>("threshold") ?: 0.0
                    val noiseFloor = call.argument<Double>("noiseFloor") ?: 0.0
                    val speaking = call.argument<Boolean>("speaking") ?: false
                    OverlayService.updateState(
                        names,
                        transmitting,
                        connected,
                        muted,
                        deafened,
                        level.toFloat(),
                        threshold.toFloat(),
                        noiseFloor.toFloat(),
                        speaking,
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
        OverlayService.onTransmit = null
        OverlayService.onMediaButton = null
        OverlayService.onToggleMute = null
        OverlayService.onToggleDeafen = null
        OverlayService.onHangup = null
        channel?.setMethodCallHandler(null)
        buttonChannel?.setMethodCallHandler(null)
        logChannel?.setMethodCallHandler(null)
        super.onDestroy()
    }

    private companion object {
        /// Short, because logcat truncates a tag past 23 characters on older
        /// releases and a truncated tag cannot be filtered on.
        const val LOG_TAG = "MumbleWay"
    }
}
