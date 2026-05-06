package org.ratspeak.android

import android.util.Log as AndroidLog

object RatspeakDiagnostics {
    private fun flag(name: String): Boolean {
        return when (System.getenv(name)?.trim()?.lowercase()) {
            "1", "true", "yes", "on" -> true
            else -> false
        }
    }

    fun enabled(): Boolean = flag("RATSPEAK_DIAGNOSTICS")
}

object Log {
    fun v(tag: String, message: String): Int {
        if (!RatspeakDiagnostics.enabled()) return 0
        return AndroidLog.v(tag, message)
    }

    fun d(tag: String, message: String): Int {
        if (!RatspeakDiagnostics.enabled()) return 0
        return AndroidLog.d(tag, message)
    }

    fun i(tag: String, message: String): Int {
        if (!RatspeakDiagnostics.enabled()) return 0
        return AndroidLog.i(tag, message)
    }

    fun w(tag: String, message: String): Int {
        if (!RatspeakDiagnostics.enabled()) return 0
        return AndroidLog.w(tag, message)
    }

    fun e(tag: String, message: String): Int {
        if (!RatspeakDiagnostics.enabled()) return 0
        return AndroidLog.e(tag, message)
    }

    fun e(tag: String, message: String, error: Throwable?): Int {
        if (!RatspeakDiagnostics.enabled()) return 0
        return AndroidLog.e(tag, message, error)
    }
}
