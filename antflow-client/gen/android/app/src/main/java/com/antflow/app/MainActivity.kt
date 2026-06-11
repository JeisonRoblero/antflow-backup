package com.antflow.app

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import android.app.PictureInPictureParams
import android.util.Rational
import android.os.Build
import android.provider.Settings
import android.content.pm.PackageManager
import android.Manifest
import androidx.core.content.ContextCompat
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import android.media.AudioManager
import android.media.AudioDeviceInfo
import android.content.Context
import android.view.Window
import android.view.WindowManager

class MainActivity : TauriActivity() {
  private external fun sendMobilePermissions(permissions: String)
  private external fun sendMobileAudioDevices(devices: String)
  private external fun checkPipRequest(): Boolean
  private external fun checkPendingActions(): Int

  private val pipHandler = android.os.Handler(android.os.Looper.getMainLooper())
  private val pipRunnable = object : Runnable {
    override fun run() {
      try {
        if (checkPipRequest()) {
          enterPip()
        }
        val action = checkPendingActions()
        if (action == 1) {
          openOverlaySettings()
        } else if (action == 2) {
          openAssistantSettings()
        } else if (action == 3) {
          enterMinimizedOverlay()
        } else if (action == 4) {
          enterMaximizedOverlay()
        } else if (action == 5) {
          openAppDetailsSettings()
        } else if (action == 6) {
          finishAndRemoveTask()
        } else if (action == 7) {
          startDownloadService()
        } else if (action == 8) {
          pauseResumeDownloadService()
        } else if (action == 9) {
          cancelDownloadService()
        } else if (action == 10) {
          requestNotificationPermission()
        } else if (action == 11) {
          openNotificationSettings()
        }
      } catch (e: Exception) {
        e.printStackTrace()
      }
      pipHandler.postDelayed(this, 150)
    }
  }

  fun requestNotificationPermission() {
    try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        val permission = "android.permission.POST_NOTIFICATIONS"
        if (ContextCompat.checkSelfPermission(this, permission) != PackageManager.PERMISSION_GRANTED) {
          requestPermissions(arrayOf(permission), 992)
        }
      }
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  fun openNotificationSettings() {
    try {
      val intent = android.content.Intent().apply {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
          action = android.provider.Settings.ACTION_APP_NOTIFICATION_SETTINGS
          putExtra(android.provider.Settings.EXTRA_APP_PACKAGE, packageName)
        } else {
          action = android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS
          data = android.net.Uri.parse("package:$packageName")
        }
        addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
      }
      startActivity(intent)
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  fun startDownloadService() {
    try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        val permission = "android.permission.POST_NOTIFICATIONS"
        if (ContextCompat.checkSelfPermission(this, permission) != PackageManager.PERMISSION_GRANTED) {
          requestPermissions(arrayOf(permission), 992)
        }
      }
      val intent = android.content.Intent(this, ModelDownloadService::class.java).apply {
        action = ModelDownloadService.ACTION_START
      }
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        startForegroundService(intent)
      } else {
        startService(intent)
      }
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  fun pauseResumeDownloadService() {
    try {
      val intent = android.content.Intent(this, ModelDownloadService::class.java).apply {
        action = if (ModelDownloadService.isPaused) ModelDownloadService.ACTION_RESUME else ModelDownloadService.ACTION_PAUSE
      }
      startService(intent)
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  fun cancelDownloadService() {
    try {
      val intent = android.content.Intent(this, ModelDownloadService::class.java).apply {
        action = ModelDownloadService.ACTION_CANCEL
      }
      startService(intent)
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    try {
      val w: Window = getWindow()
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
        w.navigationBarColor = android.graphics.Color.TRANSPARENT
      }
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        w.setDecorFitsSystemWindows(false)
      }
      w.addFlags(WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS)
    } catch (e: Exception) {
      e.printStackTrace()
    }
    super.onCreate(savedInstanceState)

    
    try {
      System.loadLibrary("antflow")
      sendMobilePermissions(checkMobilePermissions())
      updateAudioDevices()
      
      val audioManager = getSystemService(Context.AUDIO_SERVICE) as AudioManager
      audioManager.registerAudioDeviceCallback(object : android.media.AudioDeviceCallback() {
        override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>?) {
          updateAudioDevices()
        }
        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>?) {
          updateAudioDevices()
        }
      }, null)
    } catch (e: Exception) {
      e.printStackTrace()
    }
    
    pipHandler.post(pipRunnable)
  }

  override fun onResume() {
    super.onResume()
    try {
      sendMobilePermissions(checkMobilePermissions())
      updateAudioDevices()
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    try {
      sendMobilePermissions(checkMobilePermissions())
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  fun updateAudioDevices() {
    try {
      val audioManager = getSystemService(Context.AUDIO_SERVICE) as AudioManager
      val devices = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
        audioManager.getDevices(AudioManager.GET_DEVICES_ALL)
      } else {
        emptyArray<AudioDeviceInfo>()
      }
      val json = StringBuilder("[")
      var first = true
      for (device in devices) {
        val name = when (device.type) {
          AudioDeviceInfo.TYPE_BUILTIN_MIC -> "Built-in Microphone"
          AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> "Earpiece"
          AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> "Built-in Speaker"
          AudioDeviceInfo.TYPE_WIRED_HEADSET -> "Wired Headset"
          AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> "Wired Headphones"
          AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> "Bluetooth Device"
          AudioDeviceInfo.TYPE_BLUETOOTH_A2DP -> "Bluetooth Output"
          else -> null
        }
        if (name != null) {
          if (!first) json.append(",")
          first = false
          json.append("""{"name":"$name","isInput":${device.isSource},"isOutput":${device.isSink},"id":"${device.id}"}""")
        }
      }
      json.append("]")
      sendMobileAudioDevices(json.toString())
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  private fun findWebView(view: View): WebView? {
    if (view is WebView) {
      return view
    }
    if (view is ViewGroup) {
      for (i in 0 until view.childCount) {
        val child = view.getChildAt(i)
        val result = findWebView(child)
        if (result != null) {
          return result
        }
      }
    }
    return null
  }

  private fun getWebView(): WebView? {
    return findWebView(window.decorView)
  }

  override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: android.content.res.Configuration) {
    super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
    runOnUiThread {
      try {
        val webView = getWebView()
        webView?.evaluateJavascript("document.body.classList.toggle('is-pip', $isInPictureInPictureMode);", null)
      } catch (e: Exception) {
        e.printStackTrace()
      }
    }
  }

  fun enterPip() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      try {
        val params = PictureInPictureParams.Builder()
          .setAspectRatio(Rational(239, 100))
          .build()
        enterPictureInPictureMode(params)
      } catch (e: Exception) {
        e.printStackTrace()
      }
    }
  }

  fun openOverlaySettings() {
    try {
      val intent = android.content.Intent(
        android.provider.Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
        android.net.Uri.parse("package:$packageName")
      )
      intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
      startActivity(intent)
    } catch (e: Exception) {
      try {
        val intent = android.content.Intent(android.provider.Settings.ACTION_MANAGE_OVERLAY_PERMISSION)
        intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivity(intent)
      } catch (ex: Exception) {
        ex.printStackTrace()
      }
    }
  }

  fun openAssistantSettings() {
    try {
      val intent = android.content.Intent("android.settings.ACTION_ASSISTANT_SETTINGS")
      intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
      startActivity(intent)
    } catch (e: Exception) {
      try {
        val intent = android.content.Intent(android.provider.Settings.ACTION_VOICE_INPUT_SETTINGS)
        intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivity(intent)
      } catch (ex: Exception) {
        ex.printStackTrace()
      }
    }
  }

  fun openAppDetailsSettings() {
    try {
      val intent = android.content.Intent(
        android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
        android.net.Uri.parse("package:$packageName")
      )
      intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
      startActivity(intent)
    } catch (e: Exception) {
      e.printStackTrace()
    }
  }

  fun enterMinimizedOverlay() {
    runOnUiThread {
      try {
        val lp = window.attributes
        lp.gravity = android.view.Gravity.BOTTOM
        lp.height = (72 * resources.displayMetrics.density).toInt() // Height of talkbar (e.g. 72dp)
        lp.width = android.view.WindowManager.LayoutParams.MATCH_PARENT
        window.attributes = lp
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL)
        window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        
        val webView = getWebView()
        webView?.evaluateJavascript("document.body.classList.add('is-pip');", null)
      } catch (e: Exception) {
        e.printStackTrace()
      }
    }
  }

  fun enterMaximizedOverlay() {
    runOnUiThread {
      try {
        val lp = window.attributes
        lp.gravity = android.view.Gravity.CENTER
        lp.height = android.view.WindowManager.LayoutParams.MATCH_PARENT
        lp.width = android.view.WindowManager.LayoutParams.MATCH_PARENT
        window.attributes = lp
        window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL)
        window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        
        val webView = getWebView()
        webView?.evaluateJavascript("document.body.classList.remove('is-pip');", null)
      } catch (e: Exception) {
        e.printStackTrace()
      }
    }
  }

  fun checkMobilePermissions(): String {
    val micGranted = ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
    val overlayGranted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
      Settings.canDrawOverlays(this)
    } else {
      true
    }
    
    val assistant = Settings.Secure.getString(contentResolver, "assistant")
    val isAssistant = assistant != null && assistant.contains(packageName)

    val notificationsGranted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      ContextCompat.checkSelfPermission(this, "android.permission.POST_NOTIFICATIONS") == PackageManager.PERMISSION_GRANTED
    } else {
      true
    }

    return """
      {
        "microphone": $micGranted,
        "overlay": $overlayGranted,
        "assistant": $isAssistant,
        "notifications": $notificationsGranted
      }
    """.trimIndent()
  }

  override fun onKeyDown(keyCode: Int, event: android.view.KeyEvent?): Boolean {
    if (keyCode == android.view.KeyEvent.KEYCODE_BACK) {
      val webView = getWebView()
      if (webView != null) {
        webView.evaluateJavascript("if (window.onAndroidBackReceived) { window.onAndroidBackReceived(); }", null)
        return true
      }
    }
    return super.onKeyDown(keyCode, event)
  }
}

class AssistantService : android.service.voice.VoiceInteractionService()

class AssistantSessionService : android.service.voice.VoiceInteractionSessionService() {
  override fun onNewSession(args: android.os.Bundle?): android.service.voice.VoiceInteractionSession {
    return AssistantSession(this)
  }
}

class AssistantSession(context: android.content.Context) : android.service.voice.VoiceInteractionSession(context) {
  override fun onCreate() {
    super.onCreate()
    try {
      val intent = android.content.Intent(context, MainActivity::class.java)
      intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
      context.startActivity(intent)
    } catch (e: Exception) {
      e.printStackTrace()
    }
    finish()
  }
}
