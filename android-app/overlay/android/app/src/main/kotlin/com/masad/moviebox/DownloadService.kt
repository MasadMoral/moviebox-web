package com.masad.moviebox

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.MediaScannerConnection
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.IBinder
import android.provider.MediaStore
import androidx.core.content.FileProvider
import java.io.File
import java.io.FileOutputStream
import java.io.OutputStream
import java.net.HttpURLConnection
import java.net.URL

/**
 * Foreground-service downloader with a real progress notification.
 * The downloads go through the embedded Rust server on 127.0.0.1, so the
 * service must live in this app process (it does).
 */
class DownloadService : Service() {
    companion object {
        private const val CH = "downloads"
        private const val NOTIF_PROGRESS = 101
        private const val NOTIF_DONE = 102
        private const val NOTIF_FAIL = 103

        private const val ACTION_START = "com.masad.moviebox.DL_START"
        private const val ACTION_CANCEL = "com.masad.moviebox.DL_CANCEL"
        private const val EXTRA_URL = "url"
        private const val EXTRA_NAME = "name"

        fun start(ctx: Context, url: String, name: String) {
            val i = Intent(ctx, DownloadService::class.java).apply {
                action = ACTION_START
                putExtra(EXTRA_URL, url)
                putExtra(EXTRA_NAME, name)
            }
            if (Build.VERSION.SDK_INT >= 26) {
                ctx.startForegroundService(i)
            } else {
                ctx.startService(i)
            }
        }
    }

    private val nm by lazy {
        getSystemService(NOTIFICATION_SERVICE) as NotificationManager
    }

    private val queue = ArrayDeque<Pair<String, String>>()

    @Volatile
    private var cancelled = false

    @Volatile
    private var running = false

    @Volatile
    private var conn: HttpURLConnection? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        if (Build.VERSION.SDK_INT >= 26) {
            val ch = NotificationChannel(
                CH, "Downloads", NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "MovieBox downloads"
                setShowBadge(false)
            }
            nm.createNotificationChannel(ch)
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_CANCEL -> {
                cancelled = true
                synchronized(queue) { queue.clear() }
                runCatching { conn?.disconnect() }
                nm.cancel(NOTIF_PROGRESS)
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_START -> {
                cancelled = false
                val url = intent.getStringExtra(EXTRA_URL)
                val name = intent.getStringExtra(EXTRA_NAME) ?: "video.mp4"
                if (url.isNullOrBlank()) {
                    stopSelf()
                    return START_NOT_STICKY
                }
                val first = synchronized(queue) {
                    queue.addLast(url to name)
                    queue.size == 1 && !running
                }
                if (first) {
                    startForegroundCompat(progressNotification(name, -1, 0, -1, true))
                    pump()
                }
            }
            else -> {
                stopSelf()
                return START_NOT_STICKY
            }
        }
        return START_NOT_STICKY
    }

    private fun startForegroundCompat(n: Notification) {
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(NOTIF_PROGRESS, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIF_PROGRESS, n)
        }
    }

    private fun stopForegroundCompat() {
        if (Build.VERSION.SDK_INT >= 24) {
            stopForeground(STOP_FOREGROUND_REMOVE)
        } else {
            @Suppress("DEPRECATION")
            stopForeground(true)
        }
    }

    /** Runs the next queued download, or stops the service when the queue is empty. */
    private fun pump() {
        val next = synchronized(queue) { queue.removeFirstOrNull() }
        if (next == null) {
            running = false
            stopForegroundCompat()
            stopSelf()
            return
        }
        running = true
        Thread {
            download(next.first, next.second)
            running = false
            pump()
        }.start()
    }

    private fun download(url: String, name: String) {
        var out: OutputStream? = null
        var targetUri: Uri? = null
        var targetFile: File? = null
        var mime = mimeOf(name)

        try {
            nm.notify(NOTIF_PROGRESS, progressNotification(name, -1, 0, -1, true))
            val c = (URL(url).openConnection() as HttpURLConnection).apply {
                connectTimeout = 20_000
                readTimeout = 60_000
                instanceFollowRedirects = true
                setRequestProperty("Accept", "*/*")
                setRequestProperty("User-Agent", "MovieBox-Android")
            }
            conn = c
            val code = c.responseCode
            if (code !in 200..299) throw IllegalStateException("HTTP $code")
            val total = c.getHeaderField("Content-Length")?.toLongOrNull() ?: -1L
            mime = (c.contentType ?: mime).substringBefore(';').trim().ifBlank { mime }

            if (Build.VERSION.SDK_INT >= 29) {
                val values = ContentValues().apply {
                    put(MediaStore.Downloads.DISPLAY_NAME, name)
                    put(MediaStore.Downloads.MIME_TYPE, mime)
                    put(
                        MediaStore.Downloads.RELATIVE_PATH,
                        Environment.DIRECTORY_DOWNLOADS + "/MovieBox"
                    )
                    put(MediaStore.Downloads.IS_PENDING, 1)
                }
                val uri = contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                    ?: throw IllegalStateException("Cannot create file")
                targetUri = uri
                out = contentResolver.openOutputStream(uri)
                    ?: throw IllegalStateException("Cannot open file")
            } else {
                // Legacy path: app-private external files, exposed through FileProvider.
                val dir = getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
                    ?: throw IllegalStateException("No storage")
                dir.mkdirs()
                val f = File(dir, name)
                targetFile = f
                out = FileOutputStream(f)
            }

            val buf = ByteArray(128 * 1024)
            var done = 0L
            var lastUpdate = System.currentTimeMillis()
            c.inputStream.use { input ->
                out.use { o ->
                    while (!cancelled) {
                        val read = input.read(buf)
                        if (read < 0) break
                        o.write(buf, 0, read)
                        done += read
                        val now = System.currentTimeMillis()
                        if (now - lastUpdate >= 400) {
                            lastUpdate = now
                            val pct = if (total > 0) (done * 100 / total).toInt() else -1
                            nm.notify(
                                NOTIF_PROGRESS,
                                progressNotification(name, pct, done, total, false)
                            )
                        }
                    }
                }
            }
            out = null
            if (cancelled) throw InterruptedException("cancelled")

            if (targetUri != null) {
                contentResolver.update(
                    targetUri,
                    ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) },
                    null, null
                )
            } else {
                targetFile?.let {
                    MediaScannerConnection.scanFile(
                        this, arrayOf(it.absolutePath), arrayOf(mime), null
                    )
                }
            }
            nm.notify(NOTIF_DONE, doneNotification(name, targetUri, targetFile, mime))
        } catch (e: Exception) {
            runCatching { out?.close() }
            targetUri?.let { runCatching { contentResolver.delete(it, null, null) } }
            targetFile?.let { runCatching { it.delete() } }
            if (!cancelled) {
                nm.notify(NOTIF_FAIL, failNotification(name, e.message ?: e.javaClass.simpleName))
            }
        } finally {
            runCatching { conn?.disconnect() }
            conn = null
        }
    }

    /* ---------------- notifications ---------------- */

    private fun builder(): Notification.Builder =
        if (Build.VERSION.SDK_INT >= 26) {
            Notification.Builder(this, CH)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }

    private fun flags(): Int =
        PendingIntent.FLAG_UPDATE_CURRENT or
            (if (Build.VERSION.SDK_INT >= 23) PendingIntent.FLAG_IMMUTABLE else 0)

    private fun cancelIntent(): PendingIntent {
        val i = Intent(this, DownloadService::class.java).setAction(ACTION_CANCEL)
        return PendingIntent.getService(this, 51, i, flags())
    }

    private fun appIntent(): PendingIntent {
        val i = packageManager.getLaunchIntentForPackage(packageName)
            ?: Intent(this, MainActivity::class.java)
        i.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(this, 50, i, flags())
    }

    private fun progressNotification(
        name: String,
        pct: Int,
        done: Long,
        total: Long,
        indeterminate: Boolean
    ): Notification {
        val b = builder()
        b.setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(name)
            .setContentText(
                if (indeterminate || pct < 0) {
                    if (done > 0) fmt(done) else "Starting…"
                } else {
                    "$pct% · ${fmt(done)} / ${if (total > 0) fmt(total) else "?"}"
                }
            )
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setContentIntent(appIntent())
            .setProgress(100, pct.coerceAtLeast(0), indeterminate || pct < 0)
        @Suppress("DEPRECATION")
        b.addAction(
            Notification.Action.Builder(
                android.R.drawable.ic_menu_close_clear_cancel, "Cancel", cancelIntent()
            ).build()
        )
        return b.build()
    }

    private fun openIntent(uri: Uri?, file: File?, mime: String): PendingIntent {
        var view: Intent? = null
        if (uri != null) {
            view = Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, mime)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        } else if (file != null) {
            runCatching {
                val fu = FileProvider.getUriForFile(
                    this, "$packageName.fileprovider", file
                )
                view = Intent(Intent.ACTION_VIEW)
                    .setDataAndType(fu, mime)
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
        }
        val resolved = view?.let {
            if (packageManager.resolveActivity(it, 0) != null) it else null
        }
        val target = resolved ?: (packageManager.getLaunchIntentForPackage(packageName)
            ?: Intent(this, MainActivity::class.java))
        target.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(this, 52, target, flags())
    }

    private fun doneNotification(
        name: String,
        uri: Uri?,
        file: File?,
        mime: String
    ): Notification =
        builder()
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentTitle("Download complete")
            .setContentText(name)
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setProgress(0, 0, false)
            .setContentIntent(openIntent(uri, file, mime))
            .build()

    private fun failNotification(name: String, reason: String): Notification =
        builder()
            .setSmallIcon(android.R.drawable.stat_notify_error)
            .setContentTitle("Download failed")
            .setContentText("$name · $reason")
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(openIntent(null, null, "video/*"))
            .build()

    private fun fmt(bytes: Long): String = when {
        bytes >= 1L shl 30 -> String.format("%.1f GB", bytes / (1L shl 30).toDouble())
        bytes >= 1L shl 20 -> String.format("%.0f MB", bytes / (1L shl 20).toDouble())
        bytes >= 1L shl 10 -> String.format("%.0f KB", bytes / (1L shl 10).toDouble())
        else -> "$bytes B"
    }

    private fun mimeOf(name: String): String = when {
        name.endsWith(".mkv", true) -> "video/x-matroska"
        name.endsWith(".webm", true) -> "video/webm"
        name.endsWith(".ts", true) -> "video/mp2t"
        else -> "video/mp4"
    }

    override fun onDestroy() {
        // Service torn down mid-transfer (system or cancel): abort the worker.
        if (running) {
            cancelled = true
            runCatching { conn?.disconnect() }
        }
        super.onDestroy()
    }
}
