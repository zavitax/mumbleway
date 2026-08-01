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

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

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
                    OverlayService.updateState(names, transmitting, connected)
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
        channel?.setMethodCallHandler(null)
        super.onDestroy()
    }
}
