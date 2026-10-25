package com.masad.moviebox

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Handles the play/pause/next/previous buttons on the media notification.
 * The MediaSession callbacks cover Bluetooth/AVRCP; this covers the shade actions.
 */
class MediaActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context?, intent: Intent?) {
        val controls = MainActivity.transportControls ?: return
        when (intent?.action) {
            ACTION_PLAY -> controls.play()
            ACTION_PAUSE -> controls.pause()
            ACTION_NEXT -> controls.skipToNext()
            ACTION_PREV -> controls.skipToPrevious()
            ACTION_STOP -> controls.stop()
        }
    }

    companion object {
        const val ACTION_PLAY = "com.masad.moviebox.MEDIA_PLAY"
        const val ACTION_PAUSE = "com.masad.moviebox.MEDIA_PAUSE"
        const val ACTION_NEXT = "com.masad.moviebox.MEDIA_NEXT"
        const val ACTION_PREV = "com.masad.moviebox.MEDIA_PREV"
        const val ACTION_STOP = "com.masad.moviebox.MEDIA_STOP"
    }
}
