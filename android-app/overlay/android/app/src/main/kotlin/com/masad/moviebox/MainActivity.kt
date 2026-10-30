package com.masad.moviebox

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.PictureInPictureParams
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.net.Uri
import android.os.Build
import android.util.Rational
import android.view.KeyEvent
import android.view.WindowManager
import android.widget.Toast
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import kotlin.math.roundToInt

class MainActivity : FlutterActivity() {
    private var channel: MethodChannel? = null

    // Media session (Bluetooth / headset / lock-screen controls)
    private var mediaSession: MediaSession? = null
    private var focusRequest: AudioFocusRequest? = null
    private var focusHeld = false
    private var askedNotifications = false
    private var noisyRegistered = false

    private var title = "MovieBox"
    private var subtitle = ""
    private var videoLoaded = false
    private var videoPlaying = false
    private var canNext = false
    private var canPrev = false
    private var positionMs = 0L
    private var durationMs = 0L
    private var lastNotificationKey = ""

    private val audioManager by lazy {
        getSystemService(Context.AUDIO_SERVICE) as AudioManager
    }

    companion object {
        const val CH_PLAYBACK = "playback"
        const val NOTIF_PLAYBACK = 41

        @Volatile
        var transportControls: MediaController.TransportControls? = null
    }

    private val focusListener = AudioManager.OnAudioFocusChangeListener { change ->
        when (change) {
            AudioManager.AUDIOFOCUS_LOSS,
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> command("pause")
        }
    }

    private val noisyReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            // Headphones unplugged / Bluetooth disconnected.
            command("pause")
        }
    }

    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        createPlaybackChannel()
        setupMediaSession()
        registerNoisyReceiver()
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        val ch = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "mbw/native")
        channel = ch
        ch.setMethodCallHandler { call, result ->
            when (call.method) {
                "openExternal" -> {
                    val url = call.argument<String>("url") ?: ""
                    val view = Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse(url), "video/*")
                    startActivity(Intent.createChooser(view, "Play with"))
                    result.success(true)
                }
                "download" -> {
                    val url = call.argument<String>("url") ?: ""
                    val name = (call.argument<String>("name") ?: "video.mp4")
                        .replace(Regex("[\\\\/:*?\"<>|]"), "_")
                    askNotifications()
                    DownloadService.start(this, url, name)
                    result.success(true)
                }
                "toast" -> {
                    toast(call.arguments as? String ?: "")
                    result.success(true)
                }
                "openUrl" -> {
                    val url = call.argument<String>("url") ?: ""
                    if (url.startsWith("http://") || url.startsWith("https://")) {
                        startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
                        result.success(true)
                    } else {
                        result.success(false)
                    }
                }
                "getVolume" -> result.success(getSystemVolume())
                "setVolume" -> {
                    setSystemVolume((call.arguments as? Number)?.toDouble() ?: 1.0)
                    result.success(true)
                }
                "getBrightness" -> result.success(getWindowBrightness())
                "setBrightness" -> {
                    setWindowBrightness((call.arguments as? Number)?.toDouble() ?: 0.75)
                    result.success(true)
                }
                "enterPip" -> result.success(enterPip(requirePlaying = false))
                "setFullscreen" -> {
                    setFullscreen(call.arguments == true)
                    result.success(true)
                }
                "mediaState" -> {
                    @Suppress("UNCHECKED_CAST")
                    updateMediaState(call.arguments as? Map<String, Any?>)
                    result.success(true)
                }
                else -> result.notImplemented()
            }
        }
    }

    private fun toast(msg: String) = runOnUiThread {
        Toast.makeText(this, msg, Toast.LENGTH_LONG).show()
    }

    /* ---------------- notification permission ---------------- */

    private fun askNotifications() {
        if (Build.VERSION.SDK_INT >= 33) {
            if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
                PackageManager.PERMISSION_GRANTED
            ) {
                requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 907)
            }
        }
    }

    /* ---------------- audio volume / window brightness ---------------- */

    private fun getSystemVolume(): Double {
        val max = audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(1)
        return audioManager.getStreamVolume(AudioManager.STREAM_MUSIC).toDouble() / max
    }

    private fun setSystemVolume(v: Double) {
        val max = audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
        val index = (v.coerceIn(0.0, 1.0) * max).roundToInt()
        audioManager.setStreamVolume(AudioManager.STREAM_MUSIC, index, 0)
    }

    private fun getWindowBrightness(): Double {
        val b = window.attributes.screenBrightness
        return if (b < 0) -1.0 else b.toDouble()
    }

    private fun setWindowBrightness(v: Double) {
        val attrs = window.attributes
        attrs.screenBrightness = v.coerceIn(0.01, 1.0).toFloat()
        window.attributes = attrs
    }

    /* ---------------- fullscreen orientation (allows 180° flip) ---------------- */

    private fun setFullscreen(on: Boolean) {
        requestedOrientation = if (on) {
            ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        } else {
            ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        }
    }

    /* ---------------- Picture-in-Picture ---------------- */

    private fun enterPip(requirePlaying: Boolean): Boolean {
        if (Build.VERSION.SDK_INT < 26) return false
        if (!videoLoaded) return false
        if (requirePlaying && !videoPlaying) return false
        return try {
            val params = PictureInPictureParams.Builder()
                .setAspectRatio(Rational(16, 9))
                .build()
            enterPictureInPictureMode(params)
        } catch (_: Exception) {
            false
        }
    }

    override fun onUserLeaveHint() {
        // Pressing Home while a video plays pops it out automatically.
        if (videoLoaded && videoPlaying) enterPip(requirePlaying = true)
        super.onUserLeaveHint()
    }

    override fun onPictureInPictureModeChanged(
        isInPictureInPictureMode: Boolean,
        newConfig: Configuration
    ) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        if (isInPictureInPictureMode) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
        runOnUiThread {
            channel?.invokeMethod("pipChanged", isInPictureInPictureMode)
        }
    }

    /* ---------------- hardware media keys ---------------- */

    override fun onKeyDown(keyCode: Int, event: KeyEvent?): Boolean {
        when (keyCode) {
            KeyEvent.KEYCODE_HEADSETHOOK,
            KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE -> {
                command("toggle")
                return true
            }
            KeyEvent.KEYCODE_MEDIA_PLAY -> {
                command("play")
                return true
            }
            KeyEvent.KEYCODE_MEDIA_PAUSE,
            KeyEvent.KEYCODE_MEDIA_STOP -> {
                command("pause")
                return true
            }
            KeyEvent.KEYCODE_MEDIA_NEXT -> {
                command("next")
                return true
            }
            KeyEvent.KEYCODE_MEDIA_PREVIOUS -> {
                command("prev")
                return true
            }
        }
        return super.onKeyDown(keyCode, event)
    }

    /* ---------------- media session ---------------- */

    private fun setupMediaSession() {
        mediaSession = MediaSession(this, "MovieBox").apply {
            setFlags(
                MediaSession.FLAG_HANDLES_MEDIA_BUTTONS or
                    MediaSession.FLAG_HANDLES_TRANSPORT_CONTROLS
            )
            setCallback(object : MediaSession.Callback() {
                override fun onPlay() = command("play")
                override fun onPause() = command("pause")
                override fun onStop() = command("pause")
                override fun onSkipToNext() = command("next")
                override fun onSkipToPrevious() = command("prev")
                override fun onSeekTo(pos: Long) = command("seek:$pos")
            })
            isActive = true
        }
        transportControls = mediaSession?.controller?.transportControls
    }

    private fun command(cmd: String) = runOnUiThread {
        channel?.invokeMethod("mediaCommand", cmd)
    }

    private fun createPlaybackChannel() {
        if (Build.VERSION.SDK_INT >= 26) {
            val ch = NotificationChannel(
                CH_PLAYBACK, "Playback", NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Playback controls"
                setShowBadge(false)
            }
            (getSystemService(NOTIFICATION_SERVICE) as NotificationManager)
                .createNotificationChannel(ch)
        }
    }

    private fun updateMediaState(map: Map<String, Any?>?) {
        if (map == null) return
        videoLoaded = map["hasVideo"] == true
        videoPlaying = map["playing"] == true
        canNext = map["canNext"] == true
        canPrev = map["canPrev"] == true
        title = (map["title"] as? String).orEmpty().ifBlank { "MovieBox" }
        subtitle = (map["subtitle"] as? String).orEmpty()
        positionMs = (map["position"] as? Number)?.toLong() ?: 0L
        durationMs = (map["duration"] as? Number)?.toLong() ?: 0L

        val flags = PlaybackState.ACTION_PLAY or
            PlaybackState.ACTION_PAUSE or
            PlaybackState.ACTION_PLAY_PAUSE or
            PlaybackState.ACTION_STOP or
            PlaybackState.ACTION_SEEK_TO or
            (if (canNext) PlaybackState.ACTION_SKIP_TO_NEXT else 0) or
            (if (canPrev) PlaybackState.ACTION_SKIP_TO_PREVIOUS else 0)
        val state = when {
            !videoLoaded -> PlaybackState.STATE_NONE
            videoPlaying -> PlaybackState.STATE_PLAYING
            else -> PlaybackState.STATE_PAUSED
        }
        mediaSession?.setPlaybackState(
            PlaybackState.Builder()
                .setActions(flags)
                .setState(state, positionMs, if (videoPlaying) 1f else 0f)
                .build()
        )
        mediaSession?.setMetadata(
            MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, title)
                .putString(MediaMetadata.METADATA_KEY_ARTIST, subtitle.ifBlank { "MovieBox" })
                .putLong(MediaMetadata.METADATA_KEY_DURATION, durationMs)
                .build()
        )
        mediaSession?.isActive = videoLoaded

        if (videoPlaying) {
            requestFocus()
            if (!askedNotifications) {
                askedNotifications = true
                askNotifications()
            }
        } else {
            abandonFocus()
        }
        updatePlaybackNotification()
    }

    private fun notifBuilder(): Notification.Builder =
        if (Build.VERSION.SDK_INT >= 26) {
            Notification.Builder(this, CH_PLAYBACK)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }

    private fun pendingIntentFlags(): Int =
        PendingIntent.FLAG_UPDATE_CURRENT or
            (if (Build.VERSION.SDK_INT >= 23) PendingIntent.FLAG_IMMUTABLE else 0)

    private fun mediaAction(icon: Int, label: String, action: String, req: Int): Notification.Action {
        val i = Intent(this, MediaActionReceiver::class.java).setAction(action)
        val pi = PendingIntent.getBroadcast(this, req, i, pendingIntentFlags())
        @Suppress("DEPRECATION")
        return Notification.Action.Builder(icon, label, pi).build()
    }

    private fun openAppIntent(): PendingIntent {
        val i = packageManager.getLaunchIntentForPackage(packageName)
            ?: Intent(this, MainActivity::class.java)
        i.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(this, 40, i, pendingIntentFlags())
    }

    private fun updatePlaybackNotification() {
        val nm = getSystemService(NOTIFICATION_SERVICE) as NotificationManager
        if (!videoLoaded) {
            nm.cancel(NOTIF_PLAYBACK)
            lastNotificationKey = ""
            return
        }
        val key = "$videoPlaying|$title|$subtitle|$canNext|$canPrev"
        if (key == lastNotificationKey) return
        lastNotificationKey = key

        val showPrev = canPrev
        val showNext = canNext
        val compact = if (showPrev || showNext) intArrayOf(0, 1) else intArrayOf(0)

        val b = notifBuilder()
        b.setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle(title)
            .setContentText(subtitle.ifBlank { if (videoPlaying) "Playing" else "Paused" })
            .setContentIntent(openAppIntent())
            .setOngoing(videoPlaying)
            .setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)

        if (showPrev) {
            b.addAction(
                mediaAction(
                    android.R.drawable.ic_media_previous, "Previous",
                    MediaActionReceiver.ACTION_PREV, 1
                )
            )
        }
        if (videoPlaying) {
            b.addAction(
                mediaAction(android.R.drawable.ic_media_pause, "Pause", MediaActionReceiver.ACTION_PAUSE, 2)
            )
        } else {
            b.addAction(
                mediaAction(android.R.drawable.ic_media_play, "Play", MediaActionReceiver.ACTION_PLAY, 3)
            )
        }
        if (showNext) {
            b.addAction(
                mediaAction(android.R.drawable.ic_media_next, "Next", MediaActionReceiver.ACTION_NEXT, 4)
            )
        }
        b.style = Notification.MediaStyle()
            .setMediaSession(mediaSession?.sessionToken)
            .setShowActionsInCompactView(*compact)
        nm.notify(NOTIF_PLAYBACK, b.build())
    }

    /* ---------------- audio focus ---------------- */

    private fun requestFocus() {
        if (focusHeld) return
        if (Build.VERSION.SDK_INT >= 26) {
            val attrs = AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE)
                .build()
            val req = focusRequest ?: AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(attrs)
                .setOnAudioFocusChangeListener(focusListener)
                .build()
                .also { focusRequest = it }
            focusHeld = audioManager.requestAudioFocus(req) ==
                AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        } else {
            @Suppress("DEPRECATION")
            focusHeld = audioManager.requestAudioFocus(
                focusListener, AudioManager.STREAM_MUSIC, AudioManager.AUDIOFOCUS_GAIN
            ) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        }
    }

    private fun abandonFocus() {
        if (!focusHeld) return
        focusHeld = false
        if (Build.VERSION.SDK_INT >= 26) {
            focusRequest?.let { audioManager.abandonAudioFocusRequest(it) }
        } else {
            @Suppress("DEPRECATION")
            audioManager.abandonAudioFocus(focusListener)
        }
    }

    private fun registerNoisyReceiver() {
        if (noisyRegistered) return
        val filter = IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY)
        if (Build.VERSION.SDK_INT >= 33) {
            registerReceiver(noisyReceiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            registerReceiver(noisyReceiver, filter)
        }
        noisyRegistered = true
    }

    override fun onDestroy() {
        if (noisyRegistered) {
            runCatching { unregisterReceiver(noisyReceiver) }
            noisyRegistered = false
        }
        abandonFocus()
        mediaSession?.isActive = false
        mediaSession?.release()
        mediaSession = null
        transportControls = null
        channel = null
        super.onDestroy()
    }
}
