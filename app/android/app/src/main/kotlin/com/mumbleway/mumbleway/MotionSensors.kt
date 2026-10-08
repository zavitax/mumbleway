package com.mumbleway.mumbleway

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Build
import android.util.Log

/**
 * The phone's own motion, for tap detection and for the recorder's third track.
 *
 * **Written here rather than taken from a plugin**, and the rate is the reason.
 * `sensors_plus` and its alternatives hand over a stream at whatever cadence
 * they choose; this needs an explicit one, because the whole question of whether
 * a tap can be told from a pothole turns on it. A tap's mechanical transient
 * runs 20–80 ms, so at 100 Hz it is three to eight samples and its onset is a
 * step rather than a slope; at 400 Hz the shape is visible. Android can give the
 * latter and iOS cannot, and that asymmetry is a finding rather than a detail.
 *
 * ## Three sensors, one sample
 *
 * `TYPE_LINEAR_ACCELERATION` is acceleration with gravity removed and
 * `TYPE_GRAVITY` is the part that was taken out, both produced by the
 * platform's own sensor fusion — which is better than anything worth writing
 * here, and the pair is what separates a tap from road shock: a hand reaching
 * to a thigh moves across the world vertical, where suspension travel is along
 * it.
 *
 * They arrive on separate callbacks at separate rates, so rather than
 * interpolating, the latest gravity and rotation are held and a sample is
 * emitted on each **linear acceleration** event. That is the signal the
 * detector actually works on, so it sets the clock; the other two change far
 * more slowly than a tap and a sample old is immaterial to them.
 *
 * ## The rate, and the permission
 *
 * `SENSOR_DELAY_FASTEST` asks for everything the hardware has. Above 200 Hz,
 * Android 12 requires `HIGH_SAMPLING_RATE_SENSORS` — declared in the manifest,
 * and a normal permission rather than a dangerous one, so there is nothing to
 * ask a rider. Without it the platform silently caps delivery at 200 Hz, which
 * would look exactly like a phone whose sensors are slow.
 */
class MotionSensors(context: Context) {
    private val sensors: SensorManager =
        context.getSystemService(Context.SENSOR_SERVICE) as SensorManager

    /** `(platformNanos, accel, gravity, rotation)`, each vector of three. */
    var onSample: ((Long, FloatArray, FloatArray, FloatArray) -> Unit)? = null

    private var listener: SensorEventListener? = null
    private val gravity = FloatArray(3)
    private val rotation = FloatArray(3)

    /** Whether this phone can supply what the detector needs at all. */
    fun available(): Boolean =
        sensors.getDefaultSensor(Sensor.TYPE_LINEAR_ACCELERATION) != null

    fun start() {
        if (listener != null) return
        val accel = sensors.getDefaultSensor(Sensor.TYPE_LINEAR_ACCELERATION)
        if (accel == null) {
            // Not a failure to report upwards: a phone without the fused
            // sensors simply cannot offer this feature, and the setting that
            // depends on it says so rather than failing when used.
            Log.i(TAG, "no linear-acceleration sensor; motion is unavailable")
            return
        }

        val l =
            object : SensorEventListener {
                override fun onSensorChanged(event: SensorEvent?) {
                    val e = event ?: return
                    when (e.sensor.type) {
                        Sensor.TYPE_GRAVITY -> {
                            System.arraycopy(e.values, 0, gravity, 0, 3)
                        }
                        Sensor.TYPE_GYROSCOPE -> {
                            System.arraycopy(e.values, 0, rotation, 0, 3)
                        }
                        Sensor.TYPE_LINEAR_ACCELERATION -> {
                            // The platform's own stamp, passed through
                            // unconverted. It is nanoseconds since boot, which
                            // is not the audio clock and not wall time — the
                            // recorder writes it beside an arrival stamp of its
                            // own precisely so the difference can be measured
                            // rather than assumed.
                            onSample?.invoke(
                                e.timestamp,
                                floatArrayOf(e.values[0], e.values[1], e.values[2]),
                                gravity.copyOf(),
                                rotation.copyOf(),
                            )
                        }
                    }
                }

                override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
            }
        listener = l

        // Fastest for the one that matters; the other two are asked for at a
        // slower rate on purpose, since they change far more slowly than a tap
        // and three streams at maximum rate is battery spent for nothing.
        sensors.registerListener(l, accel, SensorManager.SENSOR_DELAY_FASTEST)
        sensors.getDefaultSensor(Sensor.TYPE_GRAVITY)?.let {
            sensors.registerListener(l, it, SensorManager.SENSOR_DELAY_GAME)
        }
        sensors.getDefaultSensor(Sensor.TYPE_GYROSCOPE)?.let {
            sensors.registerListener(l, it, SensorManager.SENSOR_DELAY_GAME)
        }
        Log.i(TAG, "motion sensors started (API ${Build.VERSION.SDK_INT})")
    }

    fun stop() {
        val l = listener ?: return
        listener = null
        runCatching { sensors.unregisterListener(l) }
            .onFailure { Log.w(TAG, "could not unregister the motion listener", it) }
    }

    private companion object {
        const val TAG = "MumbleWay"
    }
}
