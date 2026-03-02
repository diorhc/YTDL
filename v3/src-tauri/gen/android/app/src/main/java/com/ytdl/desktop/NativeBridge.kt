package com.ytdl.desktop

import android.util.Log
import java.io.File

object NativeBridge {
  private const val TAG = "YTDL-NativeBridge"

  // Track whether the native library was loaded successfully
  @Volatile
  private var nativeLibLoaded = false

  init {
    // The native library "ytdl_lib" is loaded by the Tauri runtime
    // (WryActivity companion object) during Activity creation.
    // We do NOT call System.loadLibrary() here to avoid double-load issues.
    // Instead, we probe whether the library is already available by calling
    // a harmless JNI function.
    nativeLibLoaded = try {
      // nativeSetTermuxInfo is a void function that just stores two AtomicBool
      // values in Rust. Calling it with (false, false) is a safe no-op.
      nativeSetTermuxInfo(false, false)
      Log.d(TAG, "Native library probe succeeded — JNI bridge ready")
      true
    } catch (_: UnsatisfiedLinkError) {
      // Library not loaded yet — try loading it ourselves as fallback
      try {
        System.loadLibrary("ytdl_lib")
        Log.d(TAG, "Loaded ytdl_lib via System.loadLibrary() fallback")
        true
      } catch (e: Throwable) {
        Log.w(TAG, "ytdl_lib not available: ${e.message}")
        false
      }
    } catch (e: Throwable) {
      // The JNI call threw something other than UnsatisfiedLinkError —
      // this means the library IS loaded (the function was found).
      Log.d(TAG, "Native library probe threw non-link error (library IS loaded): ${e.message}")
      true
    }
  }

  /** Returns true if the native JNI library is loaded and usable. */
  fun isNativeReady(): Boolean = nativeLibLoaded

  // ── nativeLibraryDir bridge ──────────────────────────────────────────────

  @JvmStatic
  external fun nativeSetNativeLibDir(path: String): Boolean

  fun setNativeLibDir(path: String): Boolean {
    return try {
      nativeSetNativeLibDir(path)
    } catch (e: Throwable) {
      Log.w(TAG, "nativeSetNativeLibDir failed", e)
      false
    }
  }

  // ── Cache dir bridge ─────────────────────────────────────────────────────

  @JvmStatic
  external fun nativeSetCacheDir(path: String): Boolean

  // ── Termux availability bridge ───────────────────────────────────────────

  @JvmStatic
  external fun nativeSetTermuxInfo(installed: Boolean, hasPermission: Boolean): Unit

  fun setTermuxInfo(installed: Boolean, hasPermission: Boolean) {
    if (!nativeLibLoaded) {
      Log.d(TAG, "setTermuxInfo skipped: native library not loaded")
      return
    }
    try {
      nativeSetTermuxInfo(installed, hasPermission)
    } catch (e: Throwable) {
      Log.w(TAG, "nativeSetTermuxInfo failed", e)
    }
  }

  // ── Termux execution bridge (called from Rust via JNI) ──────────────────

  /**
   * Run a shell command in Termux background and redirect output to [outputPath].
   * Called by Rust's android_bridge::run_termux_check().
   */
  @JvmStatic
  fun runTermuxCheck(command: String, outputPath: String): Boolean {
    return try {
      Log.d(TAG, "runTermuxCheck: $command → $outputPath")
      TermuxBridge.runBackgroundToFile(command, File(outputPath))
    } catch (e: Throwable) {
      Log.e(TAG, "runTermuxCheck failed", e)
      false
    }
  }

  /**
   * Open Termux in foreground to run a download via generated shell script.
   * Called by Rust's android_bridge::run_termux_download().
   */
  @JvmStatic
  fun runTermuxDownload(url: String, outputDir: String, formatId: String, extraArgs: String, downloadId: String): Boolean {
    return try {
      Log.d(TAG, "runTermuxDownload: $url → $outputDir format=$formatId id=$downloadId")
      val extras = if (extraArgs.isNotEmpty()) {
        extraArgs.split("|||").filter { it.isNotEmpty() }
      } else {
        emptyList()
      }
      TermuxBridge.downloadViaTermux(url, outputDir, formatId, extras, downloadId)
    } catch (e: Throwable) {
      Log.e(TAG, "runTermuxDownload failed", e)
      false
    }
  }

  /**
   * Open Termux and run the setup/install commands.
   * Called by Rust's android_bridge::launch_termux_setup().
   */
  @JvmStatic
  fun launchTermuxSetup(): Boolean {
    return try {
      TermuxBridge.openTermuxAndInstall()
      true
    } catch (e: Throwable) {
      Log.e(TAG, "launchTermuxSetup failed", e)
      false
    }
  }

  /**
   * Open Termux app.
   * Called by Rust's android_bridge::open_termux_app().
   */
  @JvmStatic
  fun openTermuxApp(): Boolean {
    return try {
      TermuxBridge.openTermux()
    } catch (e: Throwable) {
      Log.e(TAG, "openTermuxApp failed", e)
      false
    }
  }

  /**
   * Run a foreground command in Termux terminal.
   */
  @JvmStatic
  fun runTermuxForeground(command: String, args: String, outputDir: String): Boolean {
    return try {
      val argsArray = if (args.isNotEmpty()) {
        args.split("|||").filter { it.isNotEmpty() }.toTypedArray()
      } else {
        emptyArray()
      }
      TermuxBridge.runInTerminalForeground(command, argsArray, outputDir)
    } catch (e: Throwable) {
      Log.e(TAG, "runTermuxForeground failed", e)
      false
    }
  }

  // ── Storage permission bridge (called from Rust via JNI) ────────────────

  /**
   * Check if the app has full external storage access (MANAGE_EXTERNAL_STORAGE on 11+).
   * Called by Rust's android_bridge::check_storage_permission().
   */
  @JvmStatic
  fun checkStoragePermission(): Boolean {
    return try {
      TermuxBridge.hasStoragePermission()
    } catch (e: Throwable) {
      Log.e(TAG, "checkStoragePermission failed", e)
      false
    }
  }

  /**
   * Open Settings to grant MANAGE_EXTERNAL_STORAGE.
   * Called by Rust's android_bridge::request_storage_permission().
   */
  @JvmStatic
  fun requestStoragePermission(): Boolean {
    return try {
      TermuxBridge.requestStoragePermission()
    } catch (e: Throwable) {
      Log.e(TAG, "requestStoragePermission failed", e)
      false
    }
  }

  /**
   * Open a URL in the system browser via Android Intent.
   * Called by Rust's android_bridge::open_url().
   */
  @JvmStatic
  fun openUrl(url: String): Boolean {
    return try {
      TermuxBridge.openUrl(url)
    } catch (e: Throwable) {
      Log.e(TAG, "openUrl failed", e)
      false
    }
  }

  /**
   * Open a local file or directory on Android.
   * Called by Rust's android_bridge::open_file_path().
   */
  @JvmStatic
  fun openFilePath(path: String): Boolean {
    return try {
      TermuxBridge.openFilePath(path)
    } catch (e: Throwable) {
      Log.e(TAG, "openFilePath failed", e)
      false
    }
  }
}
