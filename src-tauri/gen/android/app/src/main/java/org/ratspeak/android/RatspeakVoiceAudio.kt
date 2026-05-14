package org.ratspeak.android

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import kotlin.math.max
import kotlin.math.min

object RatspeakVoiceAudio {
    private const val BYTES_PER_FLOAT_SAMPLE = 4
    private const val TARGET_BUFFER_MS = 220
    private val lock = Any()

    private var track: AudioTrack? = null
    private var trackSampleRate = 0
    private var trackChannels = 0

    @JvmStatic
    fun start(sampleRate: Int, channels: Int): Boolean {
        val safeSampleRate = sampleRate.coerceIn(8_000, 48_000)
        val safeChannels = channels.coerceIn(1, 2)
        synchronized(lock) {
            val existing = track
            if (
                existing != null &&
                existing.state == AudioTrack.STATE_INITIALIZED &&
                trackSampleRate == safeSampleRate &&
                trackChannels == safeChannels
            ) {
                return try {
                    existing.play()
                    true
                } catch (_: Throwable) {
                    stopLocked()
                    false
                }
            }

            stopLocked()
            val channelMask = if (safeChannels == 1) {
                AudioFormat.CHANNEL_OUT_MONO
            } else {
                AudioFormat.CHANNEL_OUT_STEREO
            }
            val minBuffer = AudioTrack.getMinBufferSize(
                safeSampleRate,
                channelMask,
                AudioFormat.ENCODING_PCM_FLOAT
            )
            if (minBuffer <= 0) return false

            val targetBufferBytes = max(
                minBuffer * 2,
                safeSampleRate * safeChannels * BYTES_PER_FLOAT_SAMPLE * TARGET_BUFFER_MS / 1000
            )
            val attrs = AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                .build()
            val format = AudioFormat.Builder()
                .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                .setSampleRate(safeSampleRate)
                .setChannelMask(channelMask)
                .build()

            val created = try {
                AudioTrack.Builder()
                    .setAudioAttributes(attrs)
                    .setAudioFormat(format)
                    .setBufferSizeInBytes(targetBufferBytes)
                    .setTransferMode(AudioTrack.MODE_STREAM)
                    .build()
            } catch (_: Throwable) {
                null
            }
            if (created == null || created.state != AudioTrack.STATE_INITIALIZED) {
                try { created?.release() } catch (_: Throwable) {}
                return false
            }

            return try {
                created.setVolume(AudioTrack.getMaxVolume())
                created.play()
                track = created
                trackSampleRate = safeSampleRate
                trackChannels = safeChannels
                true
            } catch (_: Throwable) {
                try { created.release() } catch (_: Throwable) {}
                track = null
                trackSampleRate = 0
                trackChannels = 0
                false
            }
        }
    }

    @JvmStatic
    fun write(samples: FloatArray, length: Int): Int {
        synchronized(lock) {
            val active = track ?: return -1
            val count = min(length.coerceAtLeast(0), samples.size)
            if (count == 0) return 0
            return try {
                active.write(samples, 0, count, AudioTrack.WRITE_NON_BLOCKING)
            } catch (_: Throwable) {
                -1
            }
        }
    }

    @JvmStatic
    fun stop() {
        synchronized(lock) { stopLocked() }
    }

    private fun stopLocked() {
        val current = track ?: return
        track = null
        trackSampleRate = 0
        trackChannels = 0
        try { current.pause() } catch (_: Throwable) {}
        try { current.flush() } catch (_: Throwable) {}
        try { current.stop() } catch (_: Throwable) {}
        try { current.release() } catch (_: Throwable) {}
    }
}
