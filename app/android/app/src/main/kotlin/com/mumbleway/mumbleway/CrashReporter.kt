package com.mumbleway.mumbleway

import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import java.io.File
import java.io.PrintWriter
import java.io.StringWriter
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Catches what would otherwise close the app without a word.
 *
 * An uncaught exception on Android takes the process down and leaves the rider
 * with a system dialog that says the app stopped, or on many builds with
 * nothing at all — the screen simply returns to wherever it came from. Either
 * way the one thing that would explain it, the stack trace, goes only to logcat
 * and is gone the moment the device is unplugged. On a motorcycle, where the
 * phone is in a mount and the laptop is at home, that is the same as no
 * information.
 *
 * So the trace is written down before the process dies, and shown. Written
 * first, because showing it involves starting an activity and that is the part
 * most likely to fail in a process that is already broken; a report on disk is
 * read out at the next launch even if nothing could be displayed at the time.
 *
 * What this cannot catch: a crash in native code. A Rust panic that aborts, or
 * a segmentation fault, kills the process below the JVM and never reaches a
 * Java handler. The Rust side logs its own panics into the engine log instead.
 */
object CrashReporter {

    private const val TAG = "MumbleWay"

    /** Where the most recent report is kept until it has been shown. */
    private const val FILE_NAME = "last-crash.txt"

    fun install(context: Context) {
        val app = context.applicationContext
        val previous = Thread.getDefaultUncaughtExceptionHandler()

        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            val report = try {
                describe(app, thread, error).also { save(app, it) }
            } catch (secondary: Throwable) {
                // Never let the reporter be the reason nothing is reported.
                Log.e(TAG, "the crash reporter itself failed", secondary)
                null
            }

            try {
                if (report != null) show(app, report)
            } catch (secondary: Throwable) {
                Log.e(TAG, "could not show the crash report", secondary)
            }

            // Hand back to whatever was installed before — Flutter's own
            // handler, and the platform's underneath it — so the crash is
            // still reported everywhere it would have been. That call is what
            // ends the process.
            previous?.uncaughtException(thread, error)
                ?: run {
                    android.os.Process.killProcess(android.os.Process.myPid())
                    kotlin.system.exitProcess(10)
                }
        }
    }

    /** The report from a previous run, if one was never shown. */
    fun pending(context: Context): String? {
        val file = File(context.filesDir, FILE_NAME)
        return if (file.exists()) file.readText().ifBlank { null } else null
    }

    fun clear(context: Context) {
        File(context.filesDir, FILE_NAME).delete()
    }

    /**
     * The reason first, then where, then the whole chain.
     *
     * The first line is what somebody reads out over the phone, so it carries
     * the exception type and its message rather than making them scroll. The
     * causes follow because the top frame of a wrapped exception is usually
     * the wrapper and says nothing about what actually went wrong.
     */
    private fun describe(context: Context, thread: Thread, error: Throwable): String {
        val when_ = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US).format(Date())
        val trace = StringWriter().also { error.printStackTrace(PrintWriter(it)) }

        return buildString {
            appendLine("${error.javaClass.name}: ${error.message ?: "no message"}")
            appendLine()
            appendLine("Thread:  ${thread.name}")
            appendLine("When:    $when_")
            appendLine("App:     ${version(context)}")
            appendLine("Device:  ${Build.MANUFACTURER} ${Build.MODEL}")
            appendLine("Android: ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})")
            appendLine()
            append(trace.toString())
        }
    }

    /// Asked of the package manager rather than read from `BuildConfig`.
    ///
    /// That class is not generated unless the build opts in, and turning it on
    /// across the whole module to print one line in a crash report is a build
    /// change for a diagnostic. This works whatever the build settings are.
    private fun version(context: Context): String = try {
        val info = context.packageManager.getPackageInfo(context.packageName, 0)
        val code = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            info.longVersionCode
        } else {
            @Suppress("DEPRECATION")
            info.versionCode.toLong()
        }
        "${info.versionName} ($code)"
    } catch (_: Throwable) {
        // Not worth failing a crash report over.
        "unknown"
    }

    private fun save(context: Context, report: String) {
        File(context.filesDir, FILE_NAME).writeText(report)
        // Also to logcat, where a developer with a cable will look first.
        Log.e(TAG, "uncaught exception\n$report")
    }

    private fun show(context: Context, report: String) {
        context.startActivity(
            Intent(context, CrashActivity::class.java).apply {
                putExtra(CrashActivity.EXTRA_REPORT, report)
                // A new task, because the one this crashed in is going away
                // with the process.
                //
                // Deliberately not CLEAR_TASK. That ties the new activity to
                // the task being cleared, which is the one the dying process
                // owns — so the report was torn down along with it, a fraction
                // of a second after appearing. Its own affinity, declared in
                // the manifest, keeps it out of that.
                addFlags(
                    Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION,
                )
            },
        )
    }
}
