package com.ytdl.desktop

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import java.io.File

class MainActivity : TauriActivity() {
  private val tag = "YTDL-MainActivity"

  // Track whether the Rust/Tauri side has been fully initialized.
  // Set to true ONLY after all JNI bridge calls in onCreate() are complete.
  @Volatile
  private var tauriInitialized = false

  // Storage permission request launcher
  private val storagePermissionLauncher = registerForActivityResult(
    ActivityResultContracts.RequestMultiplePermissions()
  ) { permissions ->
    permissions.forEach { (perm, granted) ->
      Log.d(tag, "Permission $perm granted=$granted")
    }
  }

  // Termux RUN_COMMAND permission launcher — re-init bridge after result
  private val termuxPermissionLauncher = registerForActivityResult(
    ActivityResultContracts.RequestPermission()
  ) { granted ->
    Log.i(tag, "Termux RUN_COMMAND permission granted=$granted")
    // Re-initialize TermuxBridge so Rust gets the updated permission state.
    // Always use refreshPermissionState first (safe without JNI), then try
    // the full init only if we know JNI is ready.
    try {
      TermuxBridge.refreshPermissionState(this)
      if (tauriInitialized && NativeBridge.isNativeReady()) {
        TermuxBridge.init(this)
      }
    } catch (e: Throwable) {
      Log.w(tag, "Failed to re-init TermuxBridge after permission grant", e)
      // Non-fatal: the bridge state will be refreshed on next app restart
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    // Install the global uncaught exception handler FIRST — before ANY other code
    // runs — so that crashes during Tauri/WebView init are visible in logcat.
    val defaultHandler = Thread.getDefaultUncaughtExceptionHandler()
    Thread.setDefaultUncaughtExceptionHandler { thread, throwable ->
      Log.e(tag, "FATAL UNCAUGHT EXCEPTION on thread ${thread.name}", throwable)
      // Delegate to the default handler (shows crash dialog / kills the app)
      defaultHandler?.uncaughtException(thread, throwable)
    }

    try {
      enableEdgeToEdge()
    } catch (e: Throwable) {
      Log.w(tag, "enableEdgeToEdge() failed, continuing without it", e)
    }

    // super.onCreate() triggers the Tauri/WebView/Rust initialization.
    // If Rust setup() panics, the native crash will be caught by the
    // UncaughtExceptionHandler we just installed.
    super.onCreate(savedInstanceState)
    Log.i(tag, "super.onCreate() completed — Tauri runtime ready")

    try {
      // 1. Pass nativeLibraryDir to Rust (needed for bundled binary execution)
      val libDir = applicationInfo.nativeLibraryDir
      if (NativeBridge.isNativeReady()) {
        if (!NativeBridge.setNativeLibDir(libDir)) {
          Log.w(tag, "JNI bridge returned false for nativeLibDir")
        }
      } else {
        Log.w(tag, "JNI bridge unavailable for nativeLibDir, using file fallback only")
      }
      writeNativeLibDir(libDir)

      // 2. Pass cache dir to Rust (avoids unreliable /proc parsing on Android)
      try {
        if (NativeBridge.isNativeReady()) {
          val cacheDir = cacheDir.absolutePath
          NativeBridge.nativeSetCacheDir(cacheDir)
        }
      } catch (e: Throwable) {
        Log.w(tag, "Failed to set cache dir via JNI", e)
      }

      // 3. Initialize Termux bridge (detects Termux, passes info to Rust)
      try {
        TermuxBridge.init(this)
      } catch (e: Throwable) {
        Log.e(tag, "TermuxBridge.init() failed — Termux features disabled", e)
      }

      // Mark as fully initialized AFTER all JNI bridge calls
      tauriInitialized = true

      // 4. Request storage permissions so downloads are user-accessible
      requestStoragePermissionsIfNeeded()

      // 5. Request Termux RUN_COMMAND permission if Termux is installed but permission not granted
      requestTermuxPermissionIfNeeded()
    } catch (e: Throwable) {
      Log.e(tag, "Error during onCreate initialization", e)
    }
  }

  private fun writeNativeLibDir(libDir: String) {
    try {
      File(filesDir, "native_lib_dir.txt").writeText(libDir)
    } catch (_: Exception) {
      // Non-critical — Rust falls back to /proc/self/maps detection
    }
  }

  private fun requestTermuxPermissionIfNeeded() {
    if (!TermuxBridge.isInstalled) return
    if (TermuxBridge.hasRunPermission) return

    val perm = "com.termux.permission.RUN_COMMAND"
    if (ContextCompat.checkSelfPermission(this, perm) != PackageManager.PERMISSION_GRANTED) {
      Log.i(tag, "Requesting Termux RUN_COMMAND permission at runtime")
      try {
        termuxPermissionLauncher.launch(perm)
      } catch (e: Exception) {
        // Some Android versions/ROMs may not support requesting cross-app permissions
        Log.w(tag, "Could not request Termux permission at runtime: ${e.message}")
      }
    }
  }

  private fun requestStoragePermissionsIfNeeded() {
    val permsToRequest = mutableListOf<String>()

    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      // Android 13+: granular media permissions
      for (perm in listOf(
        Manifest.permission.READ_MEDIA_VIDEO,
        Manifest.permission.READ_MEDIA_AUDIO,
        Manifest.permission.READ_MEDIA_IMAGES
      )) {
        if (ContextCompat.checkSelfPermission(this, perm) != PackageManager.PERMISSION_GRANTED) {
          permsToRequest.add(perm)
        }
      }
    } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
      // Android 6–12: classic storage permissions
      for (perm in listOf(
        Manifest.permission.READ_EXTERNAL_STORAGE,
        Manifest.permission.WRITE_EXTERNAL_STORAGE
      )) {
        if (ContextCompat.checkSelfPermission(this, perm) != PackageManager.PERMISSION_GRANTED) {
          permsToRequest.add(perm)
        }
      }
    }

    if (permsToRequest.isNotEmpty()) {
      Log.i(tag, "Requesting storage permissions: $permsToRequest")
      storagePermissionLauncher.launch(permsToRequest.toTypedArray())
    }
  }
}
