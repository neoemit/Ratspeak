package org.ratspeak.android

import android.bluetooth.le.AdvertiseCallback
import android.bluetooth.le.AdvertiseSettings

class RatspeakAdvertiseCallback : AdvertiseCallback() {
    override fun onStartSuccess(settingsInEffect: AdvertiseSettings?) {
        Log.i("Ratspeak", "BLE advertising started successfully")
    }

    override fun onStartFailure(errorCode: Int) {
        val reason = when (errorCode) {
            ADVERTISE_FAILED_DATA_TOO_LARGE -> "data too large"
            ADVERTISE_FAILED_TOO_MANY_ADVERTISERS -> "too many advertisers"
            ADVERTISE_FAILED_ALREADY_STARTED -> "already started"
            ADVERTISE_FAILED_INTERNAL_ERROR -> "internal error"
            ADVERTISE_FAILED_FEATURE_UNSUPPORTED -> "feature unsupported"
            else -> "unknown ($errorCode)"
        }
        Log.e("Ratspeak", "BLE advertising failed: $reason")
    }
}
