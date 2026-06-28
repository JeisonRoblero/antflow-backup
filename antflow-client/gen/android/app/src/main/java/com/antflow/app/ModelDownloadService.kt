package com.antflow.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL

class ModelDownloadService : Service() {

    companion object {
        const val CHANNEL_ID = "antflow_download_channel"
        const val NOTIFICATION_ID = 991
        
        const val ACTION_START = "com.antflow.app.ACTION_START"
        const val ACTION_PAUSE = "com.antflow.app.ACTION_PAUSE"
        const val ACTION_RESUME = "com.antflow.app.ACTION_RESUME"
        const val ACTION_CANCEL = "com.antflow.app.ACTION_CANCEL"

        const val TOTAL_SIZE_BYTES = 695242752L // 695.2 MB
        
        @Volatile var isPaused = false
        @Volatile var isCancelled = false
        
        var lastPercent = 0
        var lastBytesFetched = 0L
        
        val pauseLock = Object()
    }

    private var downloadThread: Thread? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = intent?.action
        if (action == ACTION_START) {
            isCancelled = false
            isPaused = false
            startForeground(NOTIFICATION_ID, buildNotification("Starting download...", 0))
            if (downloadThread == null || !downloadThread!!.isAlive) {
                downloadThread = Thread { runDownloadLoop() }
                downloadThread!!.start()
            }
        } else if (action == ACTION_PAUSE) {
            isPaused = true
            updateNotification("Download paused", lastPercent)
            NativeBridge.updateDownloadProgress(lastPercent, "Download paused", lastBytesFetched, TOTAL_SIZE_BYTES)
        } else if (action == ACTION_RESUME) {
            isPaused = false
            synchronized(pauseLock) {
                pauseLock.notifyAll()
            }
            updateNotification("Resuming download...", lastPercent)
            NativeBridge.updateDownloadProgress(lastPercent, "Resuming download...", lastBytesFetched, TOTAL_SIZE_BYTES)
        } else if (action == ACTION_CANCEL) {
            isCancelled = true
            isPaused = false
            synchronized(pauseLock) {
                pauseLock.notifyAll()
            }
            NativeBridge.updateDownloadProgress(-1, "Cancelled", 0L, TOTAL_SIZE_BYTES)
            stopForeground(true)
            stopSelf()
        }
        return START_NOT_STICKY
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val serviceChannel = NotificationChannel(
                CHANNEL_ID,
                "Model Downloader Service",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Shows progress of local AI model download"
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager?.createNotificationChannel(serviceChannel)
        }
    }

    private fun buildNotification(statusText: String, progressPercent: Int): Notification {
        // Main intent to reopen MainActivity when notification clicked
        val pm = packageManager
        val intent = pm.getLaunchIntentForPackage(packageName)
        val pendingIntent = PendingIntent.getActivity(
            this, 0, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) PendingIntent.FLAG_IMMUTABLE else 0)
        )

        // Pause/Resume Actions
        val pauseResumeAction = if (isPaused) {
            val resumeIntent = Intent(this, ModelDownloadService::class.java).apply { action = ACTION_RESUME }
            val resumePendingIntent = PendingIntent.getService(
                this, 1, resumeIntent,
                PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) PendingIntent.FLAG_IMMUTABLE else 0)
            )
            NotificationCompat.Action.Builder(
                android.R.drawable.ic_media_play, "Resume", resumePendingIntent
            ).build()
        } else {
            val pauseIntent = Intent(this, ModelDownloadService::class.java).apply { action = ACTION_PAUSE }
            val pausePendingIntent = PendingIntent.getService(
                this, 2, pauseIntent,
                PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) PendingIntent.FLAG_IMMUTABLE else 0)
            )
            NotificationCompat.Action.Builder(
                android.R.drawable.ic_media_pause, "Pause", pausePendingIntent
            ).build()
        }

        // Cancel Action
        val cancelIntent = Intent(this, ModelDownloadService::class.java).apply { action = ACTION_CANCEL }
        val cancelPendingIntent = PendingIntent.getService(
            this, 3, cancelIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) PendingIntent.FLAG_IMMUTABLE else 0)
        )
        val cancelAction = NotificationCompat.Action.Builder(
            android.R.drawable.ic_menu_close_clear_cancel, "Cancel", cancelPendingIntent
        ).build()

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Setting Up Local AI")
            .setContentText(statusText)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .setProgress(100, progressPercent, false)
            .addAction(pauseResumeAction)
            .addAction(cancelAction)
            .setOnlyAlertOnce(true)
            .build()
    }

    private fun updateNotification(statusText: String, progressPercent: Int) {
        val manager = getSystemService(NotificationManager::class.java)
        manager?.notify(NOTIFICATION_ID, buildNotification(statusText, progressPercent))
    }

    private fun runDownloadLoop() {
        startForeground(NOTIFICATION_ID, buildNotification("Starting download...", 0))

        val modelDir = File(filesDir, "llama_model")
        if (!modelDir.exists()) {
            modelDir.mkdirs()
        }

        val wasmUrl = "https://raw.githubusercontent.com/mlc-ai/binary-mlc-llm-libs/main/web-llm-models/v0_2_84/base/Llama-3.2-1B-Instruct-q4f16_1_cs1k-webgpu.wasm"
        val modelBaseUrl = "https://huggingface.co/mlc-ai/Llama-3.2-1B-Instruct-q4f16_1-MLC/resolve/main/"
        
        val filesToDownload = mutableListOf<Pair<String, String>>()
        filesToDownload.add(Pair(wasmUrl, "Llama-3.2-1B-Instruct-q4f16_1_cs1k-webgpu.wasm"))
        
        val modelFiles = listOf(
            "ndarray-cache.json",
            "mlc-chat-config.json",
            "tensor-cache.json",
            "tokenizer.json",
            "tokenizer_config.json"
        ) + (0..21).map { "params_shard_$it.bin" }

        for (filename in modelFiles) {
            filesToDownload.add(Pair(modelBaseUrl + filename, filename))
        }

        var totalBytesFetched = 0L
        lastBytesFetched = 0L
        lastPercent = 0

        for ((index, item) in filesToDownload.withIndex()) {
            val (urlStr, filename) = item
            val destFile = File(modelDir, filename)

            var success = false
            var retryCount = 0

            while (!success && !isCancelled) {
                // Pause handler
                if (isPaused) {
                    synchronized(pauseLock) {
                        try {
                            pauseLock.wait()
                        } catch (e: InterruptedException) {
                            // Woken up
                        }
                    }
                }

                if (isCancelled) break

                try {
                    val url = URL(urlStr)
                    val conn = url.openConnection() as HttpURLConnection
                    conn.connectTimeout = 15000
                    conn.readTimeout = 20000

                    val localSize = destFile.length()
                    var rangeSupported = false

                    // If it is a binary/wasm file and exists locally, verify size and do range request
                    if ((filename.endsWith(".bin") || filename.endsWith(".wasm")) && localSize > 0) {
                        conn.setRequestProperty("Range", "bytes=$localSize-")
                    }

                    conn.connect()
                    val responseCode = conn.responseCode

                    var isAppend = false
                    var inputStream: InputStream? = null
                    var contentLength = 0L

                    if (responseCode == HttpURLConnection.HTTP_PARTIAL) {
                        isAppend = true
                        inputStream = conn.inputStream
                        contentLength = conn.contentLength.toLong() + localSize
                        rangeSupported = true
                    } else if (responseCode == HttpURLConnection.HTTP_OK) {
                        isAppend = false
                        inputStream = conn.inputStream
                        contentLength = conn.contentLength.toLong()
                    } else {
                        throw Exception("HTTP Error response: $responseCode")
                    }

                    // Check if file is already completed
                    if (isAppend && localSize >= contentLength) {
                        totalBytesFetched += contentLength
                        lastBytesFetched = totalBytesFetched
                        success = true
                        conn.disconnect()
                        break
                    }

                    val fos = FileOutputStream(destFile, isAppend)
                    val buffer = ByteArray(4096)
                    var bytesRead: Int
                    var currentFileFetched = if (isAppend) localSize else 0L

                    if (!isAppend) {
                        // Reset progress baseline for this file
                        currentFileFetched = 0
                    }

                    var lastNotifyTime = System.currentTimeMillis()

                    while (inputStream.read(buffer).also { bytesRead = it } != -1) {
                        if (isPaused) {
                            fos.flush()
                            fos.close()
                            conn.disconnect()
                            
                            synchronized(pauseLock) {
                                try {
                                    pauseLock.wait()
                                } catch (e: InterruptedException) {}
                            }
                            
                            // Reconnect
                            break // Re-enter loop to request new Range connection
                        }

                        if (isCancelled) {
                            fos.flush()
                            fos.close()
                            conn.disconnect()
                            break
                        }

                        fos.write(buffer, 0, bytesRead)
                        currentFileFetched += bytesRead
                        
                        val bytesSinceLast = if (isAppend) {
                            totalBytesFetched + (currentFileFetched - localSize)
                        } else {
                            totalBytesFetched + currentFileFetched
                        }
                        
                        lastBytesFetched = bytesSinceLast

                        val pct = Math.max(0, Math.min(100, (bytesSinceLast * 100 / TOTAL_SIZE_BYTES).toInt()))
                        lastPercent = pct

                        val now = System.currentTimeMillis()
                        if (now - lastNotifyTime > 800) {
                            lastNotifyTime = now
                            updateNotification("Downloading $filename ($pct%)", pct)
                            NativeBridge.updateDownloadProgress(pct, "Downloading $filename...", bytesSinceLast, TOTAL_SIZE_BYTES)
                        }
                    }

                    if (!isPaused && !isCancelled) {
                        fos.flush()
                        fos.close()
                        inputStream.close()
                        totalBytesFetched += if (isAppend) (contentLength - localSize) else contentLength
                        lastBytesFetched = totalBytesFetched
                        success = true
                    }
                    
                    conn.disconnect()

                } catch (e: Exception) {
                    e.printStackTrace()
                    retryCount++
                    val errorMsg = "Connection lost. Retrying ($retryCount)..."
                    updateNotification(errorMsg, lastPercent)
                    NativeBridge.updateDownloadProgress(lastPercent, errorMsg, lastBytesFetched, TOTAL_SIZE_BYTES)
                    try {
                        Thread.sleep(3000)
                    } catch (ex: InterruptedException) {}
                }
            }

            if (isCancelled) break
        }

        if (!isCancelled) {
            updateNotification("Download complete!", 100)
            NativeBridge.updateDownloadProgress(100, "Completed", TOTAL_SIZE_BYTES, TOTAL_SIZE_BYTES)
            
            // Wait 1.5 seconds before stopping service foreground task
            try {
                Thread.sleep(1500)
            } catch (e: Exception) {}
            
            stopForeground(true)
            stopSelf()
        }
    }

    override fun onDestroy() {
        isCancelled = true
        isPaused = false
        synchronized(pauseLock) {
            pauseLock.notifyAll()
        }
        super.onDestroy()
    }
}
