# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# ── YTDL: Keep JNI bridge classes ──────────────────────────────────────────
# NativeBridge has @JvmStatic external (JNI) methods. Rust looks up the class
# by its full name (com/ytdl/desktop/NativeBridge) and calls static methods.
# If R8 renames or strips the class, JNI calls crash with ClassNotFoundException.
-keep class com.ytdl.desktop.NativeBridge { *; }

# TermuxBridge is called from NativeBridge and holds the Termux integration logic.
-keep class com.ytdl.desktop.TermuxBridge { *; }

# MainActivity is referenced in the manifest but also has JNI-related init code.
-keep class com.ytdl.desktop.MainActivity { *; }

# Keep Tauri activity classes
-keep class com.ytdl.desktop.TauriActivity { *; }
-keep class * extends com.ytdl.desktop.TauriActivity { *; }

# ── Tauri / WebView bridge ─────────────────────────────────────────────────
# Tauri uses reflection for plugin initialization and IPC
-keep class app.tauri.** { *; }
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

# ── Kotlin compiler synthetic classes ──────────────────────────────────────
# Kotlin generates synthetic $$$$$NON_LOCAL_RETURN$$$$$ class for non-local
# returns in inline lambdas.  R8 cannot resolve it — safe to suppress.
-dontwarn '$$$$$NON_LOCAL_RETURN$$$$$'

# ── JNI native methods ─────────────────────────────────────────────────────
# Keep all native method declarations (JNI linkage depends on exact names)
-keepclasseswithmembernames class * {
    native <methods>;
}