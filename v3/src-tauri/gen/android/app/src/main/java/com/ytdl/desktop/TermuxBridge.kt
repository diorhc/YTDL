package com.ytdl.desktop

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
import android.util.Log
import java.io.File

/**
 * TermuxBridge: Integrates the YTDL v3 app with the Termux terminal emulator
 * using Android's **Intent** API (com.termux.RUN_COMMAND).
 *
 * ## Why Intents instead of direct exec?
 *
 * On Android, SELinux type-enforcement prevents one app (`untrusted_app` domain)
 * from executing binaries that live inside another app's data directory.
 * Even with `/data/data/com.termux/files/usr/bin` in PATH the kernel will
 * deny `execve()` with EACCES.
 *
 * The correct solution is Android Intents: send a `com.termux.RUN_COMMAND`
 * intent to Termux's `RunCommandService`.  Termux runs the command in its own
 * SELinux context (where it has the needed permissions) and can write results
 * to shared/external storage so our app can read them.
 *
 * ## User prerequisites
 *
 * 1. Install Termux from **F-Droid** (NOT the Play Store — that version is outdated).
 * 2. In Termux, run: `echo 'allow-external-apps = true' >> ~/.termux/termux.properties`
 * 3. Re-start the Termux app.
 * 4. Grant storage permission to both YTDL and Termux.
 *
 * After this setup, this bridge can run commands in Termux and download videos
 * to the shared Downloads folder without root.
 */
object TermuxBridge {
    private const val TAG = "YTDL-TermuxBridge"

    const val TERMUX_PACKAGE = "com.termux"
    private const val TERMUX_SERVICE = "com.termux.app.RunCommandService"
    private const val ACTION_RUN_COMMAND = "com.termux.RUN_COMMAND"

    // Intent extras defined by Termux RunCommandService
    private const val EXTRA_COMMAND_PATH = "com.termux.RUN_COMMAND_PATH"
    private const val EXTRA_ARGUMENTS = "com.termux.RUN_COMMAND_ARGUMENTS"
    private const val EXTRA_WORKDIR = "com.termux.RUN_COMMAND_WORKDIR"
    private const val EXTRA_BACKGROUND = "com.termux.RUN_COMMAND_BACKGROUND"
    private const val EXTRA_SESSION_ACTION = "com.termux.RUN_COMMAND_SESSION_ACTION"

    // App-global Context set by MainActivity.onCreate()
    @Volatile private var appContext: Context? = null

    // Cached state set from Kotlin during init (read by Rust via JNI)
    @Volatile var isInstalled: Boolean = false
    @Volatile var hasRunPermission: Boolean = false

    // ── Initialization ─────────────────────────────────────────────────────

    /**
     * Must be called from MainActivity.onCreate() before the bridge is used.
     * Caches the Application context and probes Termux availability.
     * All detection is wrapped in try/catch to prevent any crash during init.
     */
    fun init(context: Context) {
        try {
            appContext = context.applicationContext
        } catch (e: Throwable) {
            Log.e(TAG, "Failed to get application context", e)
            return
        }

        isInstalled = try {
            detectInstalled(context)
        } catch (e: Throwable) {
            Log.e(TAG, "Failed to detect Termux installation", e)
            false
        }

        hasRunPermission = try {
            detectRunPermission(context)
        } catch (e: Throwable) {
            Log.e(TAG, "Failed to detect Termux RUN_COMMAND permission", e)
            false
        }

        Log.i(TAG, "Termux installed=$isInstalled hasRunPermission=$hasRunPermission")

        // Pass availability info to Rust via JNI (non-critical — wrap in try/catch)
        try {
            NativeBridge.setTermuxInfo(isInstalled, hasRunPermission)
        } catch (e: Throwable) {
            Log.w(TAG, "Failed to pass Termux info to Rust via JNI", e)
        }
    }

    /**
     * Lightweight permission state refresh — only updates the Kotlin-side cached
     * flags without calling JNI. Safe to call from permission result callbacks
     * when the Tauri/Rust side may not be fully initialized yet.
     * The Rust side will pick up the updated state on the next command that calls
     * `termux_info()` via JNI or when `init()` is called again.
     */
    fun refreshPermissionState(context: Context) {
        isInstalled = try {
            detectInstalled(context)
        } catch (e: Throwable) { false }

        hasRunPermission = try {
            detectRunPermission(context)
        } catch (e: Throwable) { false }

        Log.i(TAG, "Termux state refreshed (Kotlin-only): installed=$isInstalled hasRunPermission=$hasRunPermission")

        // Try to also pass to Rust, but don't crash if JNI isn't ready
        try {
            NativeBridge.setTermuxInfo(isInstalled, hasRunPermission)
        } catch (e: Throwable) {
            Log.d(TAG, "JNI not ready during permission refresh, Kotlin state updated")
        }
    }

    // ── Availability detection ─────────────────────────────────────────────

    private fun detectInstalled(context: Context): Boolean = try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.packageManager.getPackageInfo(
                TERMUX_PACKAGE, PackageManager.PackageInfoFlags.of(0)
            )
        } else {
            @Suppress("DEPRECATION")
            context.packageManager.getPackageInfo(TERMUX_PACKAGE, 0)
        }
        true
    } catch (_: PackageManager.NameNotFoundException) { false }
    catch (_: Throwable) { false }

    /**
     * The permission `com.termux.permission.RUN_COMMAND` is granted by Android's
     * permission system only if:
     *  - The permission is declared in this app's AndroidManifest (done ✓)
     *  - Termux has `allow-external-apps = true` in its properties
     *
     * Note: the permission is signature-level on older Termux builds.
     * On F-Droid Termux it is a regular `dangerous`-level permission gated by
     * the user enabling external apps in termux.properties.
     */
    private fun detectRunPermission(context: Context): Boolean {
        if (!isInstalled) return false
        return context.packageManager.checkPermission(
            "com.termux.permission.RUN_COMMAND",
            context.packageName
        ) == PackageManager.PERMISSION_GRANTED
    }

    // ── Storage permission (MANAGE_EXTERNAL_STORAGE) ───────────────────────

    /**
     * Check if the app has full external storage access.
     *
     * - Android 11+ (API 30+): requires MANAGE_EXTERNAL_STORAGE, checked via
     *   [Environment.isExternalStorageManager].
     * - Android 10 (API 29): requestLegacyExternalStorage=true in manifest gives
     *   full access when WRITE_EXTERNAL_STORAGE is granted.
     * - Android 9 and below: WRITE_EXTERNAL_STORAGE is sufficient.
     *
     * Full storage access is required because our app and Termux exchange
     * files via shared storage (/sdcard/Download/YTDL/).
     */
    fun hasStoragePermission(): Boolean {
        val ctx = appContext ?: return false
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Environment.isExternalStorageManager()
        } else {
            // Pre-Android 11: WRITE_EXTERNAL_STORAGE gives full access
            ctx.checkSelfPermission(android.Manifest.permission.WRITE_EXTERNAL_STORAGE) ==
                PackageManager.PERMISSION_GRANTED
        }
    }

    /**
     * Open the system Settings screen to grant MANAGE_EXTERNAL_STORAGE.
     * This is NOT a standard runtime permission — it requires the user
     * to manually toggle a switch in Settings.
     */
    fun requestStoragePermission(): Boolean {
        val ctx = appContext ?: return false
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val intent = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION).apply {
                    data = Uri.parse("package:${ctx.packageName}")
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                ctx.startActivity(intent)
                true
            } else {
                // Pre-Android 11: storage permission is a normal runtime permission.
                // It should be requested via the standard requestPermissions() flow
                // in the Activity. Return false to let the caller handle it.
                Log.d(TAG, "requestStoragePermission: pre-R device, use requestPermissions()")
                false
            }
        } catch (e: Exception) {
            Log.e(TAG, "requestStoragePermission failed", e)
            false
        }
    }

    // ── Command execution ──────────────────────────────────────────────────

    /**
     * Run [command] (path inside Termux usr or system) in the **foreground**
     * Termux terminal so the user can watch progress.
     *
     * The recommended way to let users run yt-dlp downloads — they see the
     * standard yt-dlp progress bar in the Termux window.
     *
     * Downloads are written to [outputDir] (must be in shared/external storage
     * so both Termux and the YTDL app can access it, e.g. /sdcard/Download/YTDL).
     */
    fun runInTerminalForeground(
        command: String,
        args: Array<String>,
        outputDir: String? = null,
        context: Context? = appContext
    ): Boolean {
        if (context == null) {
            Log.e(TAG, "runInTerminalForeground: init() not called — no context")
            return false
        }
        if (!isInstalled) { Log.w(TAG, "Termux not installed"); return false }

        return try {
            val escapedArgs = args.joinToString(" ") { shellQuote(it) }
            val fullScript = buildString {
                if (outputDir != null) {
                    append("mkdir -p ${shellQuote(outputDir)} && ")
                }
                append("$command $escapedArgs")
            }

            val intent = buildIntent(
                shCommand = fullScript,
                background = false,
                sessionAction = "0"
            )
            startService(context, intent)
        } catch (e: Exception) {
            Log.e(TAG, "runInTerminalForeground failed", e)
            false
        }
    }

    /**
     * Run a shell command in Termux in the **background** (no visible terminal
     * window).  Output is redirected to [outputFile] by the shell wrapper so
     * the caller can poll it.
     *
     * Use for quick checks, e.g. `yt-dlp --version`.
     */
    fun runBackgroundToFile(
        shellCommand: String,
        outputFile: File,
        context: Context? = appContext
    ): Boolean {
        val ctx = context ?: return false
        if (!isInstalled) return false
        return try {
            // Auto-create the parent directory inside Termux's context
            // (Termux has storage permission from termux-setup-storage).
            // Our app may lack MANAGE_EXTERNAL_STORAGE, but Termux can mkdir.
            val dir = outputFile.parentFile?.absolutePath
            val mkdirPrefix = if (dir != null) "mkdir -p ${shellQuote(dir)} && " else ""
            val redirect = "$mkdirPrefix$shellCommand > ${shellQuote(outputFile.absolutePath)} 2>&1"
            val intent = buildIntent(shCommand = redirect, background = true)
            startService(ctx, intent)
        } catch (e: Exception) {
            Log.e(TAG, "runBackgroundToFile failed", e)
            false
        }
    }

    // ── Script helper for complex downloads ───────────────────────────────

    /**
     * Write a fully self-contained yt-dlp download shell script to an external
     * storage location where Termux can read it, then open Termux to run it.
     *
     * [url] — video URL
     * [outputDir] — writable external dir (e.g. /sdcard/Download/YTDL)
     * [formatId] — yt-dlp format string, e.g. "bestvideo+bestaudio/best"
     * [extraArgs] — additional yt-dlp arguments
     */
    fun downloadViaTermux(
        url: String,
        outputDir: String,
        formatId: String = "bestvideo+bestaudio/best",
        extraArgs: List<String> = emptyList(),
        downloadId: String = "",
        context: Context? = appContext
    ): Boolean {
        val ctx = context ?: return false

        // Build the download command inline (no script file needed).
        // Previously we wrote a script to getExternalFilesDir(), but on Android 11+
        // that path (/sdcard/Android/data/<pkg>/) is inaccessible to Termux even
        // with MANAGE_EXTERNAL_STORAGE. Inlining avoids the problem entirely.
        val ytdlpPath = "/data/data/com.termux/files/usr/bin/yt-dlp"
        val extraStr = extraArgs.joinToString(" ") { shellQuote(it) }

        // Status directory for completion sentinel files.
        // After yt-dlp finishes, we write the exit code here so the Rust
        // poller can detect completion and update the app's download list.
        val statusDir = "$outputDir/.status"

        val inlineCommand = buildString {
            append("mkdir -p ${shellQuote(outputDir)} && ")
            if (downloadId.isNotEmpty()) {
                append("mkdir -p ${shellQuote(statusDir)} && ")
            }
            append("echo '[YTDL] Starting download...' && ")
            append("${shellQuote(ytdlpPath)} ")
            append("--newline --progress ")
            append("--ffmpeg-location /data/data/com.termux/files/usr/bin ")
            append("-f ${shellQuote(formatId)} ")
            append("--merge-output-format mp4 ")
            append("--write-info-json ")
            append("-o ${shellQuote("$outputDir/%(title)s.%(ext)s")} ")
            if (extraStr.isNotEmpty()) append("$extraStr ")
            append(shellQuote(url))
            // Write completion sentinel so the app can detect download finished
            if (downloadId.isNotEmpty()) {
                val statusFile = "$statusDir/$downloadId"
                append(" ; __ytdl_exit=\$? ; ")
                append("if [ \$__ytdl_exit -eq 0 ]; then ")
                // On success, write "OK" followed by the most recently modified file
                append("echo \"OK\" > ${shellQuote(statusFile)} ; ")
                // Also try to write the actual output filename
                append("ls -t ${shellQuote(outputDir)}/*.mp4 ${shellQuote(outputDir)}/*.mkv ${shellQuote(outputDir)}/*.webm ${shellQuote(outputDir)}/*.m4a ${shellQuote(outputDir)}/*.mp3 2>/dev/null | head -1 >> ${shellQuote(statusFile)} ; ")
                append("echo '[YTDL] Done!' ; ")
                append("else ")
                append("echo \"FAIL:\$__ytdl_exit\" > ${shellQuote(statusFile)} ; ")
                append("echo '[YTDL] Download failed (exit code \$__ytdl_exit)' ; ")
                append("fi")
            } else {
                append(" && echo '[YTDL] Done!'")
            }
        }

        Log.i(TAG, "downloadViaTermux: inline command (${inlineCommand.length} chars)")

        return try {
            val intent = buildIntent(
                shCommand = inlineCommand,
                background = false,
                sessionAction = "0"
            )
            startService(ctx, intent)
        } catch (e: Exception) {
            Log.e(TAG, "downloadViaTermux failed", e)
            false
        }
    }

    // ── UI helpers ─────────────────────────────────────────────────────────

    /** Open the Termux app via its launcher intent. */
    fun openTermux(context: Context? = appContext): Boolean {
        val ctx = context ?: return false
        return try {
            val intent = ctx.packageManager
                .getLaunchIntentForPackage(TERMUX_PACKAGE)
                ?.apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) }
                ?: return false
            ctx.startActivity(intent)
            true
        } catch (e: Exception) {
            Log.e(TAG, "openTermux failed", e)
            false
        }
    }

    /**
     * Open F-Droid Termux page in the system browser.
     * Termux from F-Droid is required (Play Store version is outdated).
     */
    fun openTermuxInstallPage(context: Context? = appContext) {
        val ctx = context ?: return
        try {
            val intent = Intent(Intent.ACTION_VIEW,
                Uri.parse("https://f-droid.org/packages/com.termux/")).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            ctx.startActivity(intent)
        } catch (e: Exception) {
            Log.e(TAG, "openTermuxInstallPage failed", e)
        }
    }

    /**
     * Open Termux and run the recommended one-liner to install yt-dlp & ffmpeg.
     *
     * The command is intentionally shown in the foreground so the user can
     * monitor the (potentially long) install process.
     */
    fun openTermuxAndInstall(context: Context? = appContext) {
        val ctx = context ?: return
        val setupCmd = buildString {
            append("pkg update -y && pkg upgrade -y && ")
            append("pkg install -y python ffmpeg && ")
            append("pip install -U yt-dlp && ")
            // Auto-enable external apps (safe: only appends if not already present)
            append("mkdir -p ~/.termux && ")
            append("grep -q 'allow-external-apps' ~/.termux/termux.properties 2>/dev/null || echo 'allow-external-apps=true' >> ~/.termux/termux.properties && ")
            // Grant storage access so Termux can write to /sdcard/Download/YTDL
            append("termux-setup-storage 2>/dev/null; ")
            append("echo '' && echo '========================================' && ")
            append("echo '=== Installation complete! ===' && ")
            append("echo '========================================' && ")
            append("echo '' && ")
            append("echo 'Now restart Termux completely (swipe it away from recents),' && ")
            append("echo 'then re-open YTDL and press Re-check on the Setup page.'")
        }
        if (!runInTerminalForeground("/data/data/com.termux/files/usr/bin/sh",
                arrayOf("-c", setupCmd), context = ctx)) {
            openTermux(ctx)
        }
    }

    // ── Internal helpers ───────────────────────────────────────────────────

    private fun buildIntent(
        shCommand: String,
        background: Boolean,
        sessionAction: String = "0"
    ) = Intent(ACTION_RUN_COMMAND).apply {
        setClassName(TERMUX_PACKAGE, TERMUX_SERVICE)
        putExtra(EXTRA_COMMAND_PATH, "/data/data/com.termux/files/usr/bin/sh")
        putExtra(EXTRA_ARGUMENTS, arrayOf("-c", shCommand))
        putExtra(EXTRA_WORKDIR, "/data/data/com.termux/files/home")
        putExtra(EXTRA_BACKGROUND, background)
        if (!background) putExtra(EXTRA_SESSION_ACTION, sessionAction)
    }

    private fun startService(context: Context, intent: Intent): Boolean {
        return try {
            // Verify Termux package is still visible before sending intent.
            // On Android 11+ with missing <queries>, the system may silently
            // fail to resolve the target component.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                @Suppress("DEPRECATION")
                val resolved = context.packageManager.resolveService(intent, 0)
                if (resolved == null) {
                    Log.e(TAG, "startService: Termux RunCommandService not resolvable. " +
                        "Is Termux installed and visible?")
                    return false
                }
            }

            // IMPORTANT: Always use startService(), NOT startForegroundService().
            // Termux's RunCommandService is managed by Termux itself. Using
            // startForegroundService() for another app's service causes
            // ForegroundServiceStartNotAllowedException on Android 12+ and
            // ForegroundServiceDidNotStartInTimeException on Android 8+.
            context.startService(intent)
            true
        } catch (e: SecurityException) {
            Log.e(TAG, "startService denied — Termux external apps not enabled? " +
                "Ensure 'allow-external-apps=true' is set in ~/.termux/termux.properties", e)
            false
        } catch (e: IllegalStateException) {
            // On Android 12+, background service start might be restricted
            Log.e(TAG, "startService blocked (background restriction?)", e)
            false
        } catch (e: Exception) {
            Log.e(TAG, "startService failed", e)
            false
        }
    }

    /**
     * Open a URL in the system browser / default handler via Intent.ACTION_VIEW.
     * Used for opening YouTube links, feedback URLs, etc.
     */
    fun openUrl(url: String, context: Context? = appContext): Boolean {
        val ctx = context ?: return false
        return try {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            ctx.startActivity(intent)
            true
        } catch (e: Exception) {
            Log.e(TAG, "openUrl failed for: $url", e)
            false
        }
    }

    /**
     * Open a local file or directory using the system file manager.
     * Uses FileProvider for files (required on Android 7+) and content URI for directories.
     */
    fun openFilePath(path: String, context: Context? = appContext): Boolean {
        val ctx = context ?: return false
        return try {
            val file = File(path)
            if (!file.exists()) {
                Log.w(TAG, "openFilePath: path does not exist: $path")
                return false
            }

            if (file.isDirectory) {
                // Try opening directory in file manager via Documents UI
                try {
                    val dirPath = path.removePrefix("/sdcard/").replace("/", "%2F")
                    val intent = Intent(Intent.ACTION_VIEW).apply {
                        data = Uri.parse("content://com.android.externalstorage.documents/document/primary:$dirPath")
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    }
                    ctx.startActivity(intent)
                    return true
                } catch (_: Exception) {
                    // Fallback: open a generic file manager
                    val intent = Intent(Intent.ACTION_VIEW).apply {
                        data = Uri.fromFile(file)
                        type = "resource/folder"
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    }
                    ctx.startActivity(intent)
                    return true
                }
            }

            // For files, use FileProvider to get a content:// URI (required on Android 7+)
            val uri = androidx.core.content.FileProvider.getUriForFile(
                ctx,
                ctx.packageName + ".fileprovider",
                file
            )
            val mimeType = when {
                path.endsWith(".mp4", true) || path.endsWith(".mkv", true) ||
                path.endsWith(".webm", true) -> "video/*"
                path.endsWith(".mp3", true) || path.endsWith(".m4a", true) ||
                path.endsWith(".opus", true) || path.endsWith(".ogg", true) -> "audio/*"
                else -> "*/*"
            }
            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mimeType)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            ctx.startActivity(intent)
            true
        } catch (e: Exception) {
            Log.e(TAG, "openFilePath failed for: $path", e)
            false
        }
    }

    /** POSIX single-quote escaping: wrap in single quotes, escape embedded ones. */
    private fun shellQuote(s: String): String = "'${s.replace("'", "'\\''")}'"
}
