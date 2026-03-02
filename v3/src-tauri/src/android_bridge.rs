#[cfg(target_os = "android")]
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
#[cfg(target_os = "android")]
use jni::sys::{jboolean, JNI_FALSE, JNI_TRUE};
#[cfg(target_os = "android")]
use jni::JNIEnv;

// ── Termux availability (set from Kotlin once on startup) ─────────────────────
#[cfg(target_os = "android")]
static TERMUX_INSTALLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "android")]
static TERMUX_HAS_PERMISSION: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Cached app cache directory path (set from Kotlin via JNI)
#[cfg(target_os = "android")]
static APP_CACHE_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Cached JavaVM reference for calling Kotlin from Rust
#[cfg(target_os = "android")]
static JAVA_VM: std::sync::OnceLock<jni::JavaVM> = std::sync::OnceLock::new();

/// Cached GlobalRef to com.ytdl.desktop.NativeBridge class.
///
/// **Why this is needed:** JNI `FindClass` from threads attached via
/// `AttachCurrentThread` (like tokio worker threads) uses the **system
/// ClassLoader**, which cannot find app classes.  Only the main thread
/// (or threads started by the JVM itself) use the app ClassLoader.
///
/// We cache the class during the first JNI call from the main thread
/// (e.g. `nativeSetNativeLibDir`) and reuse the GlobalRef from all
/// subsequent Rust→Kotlin bridge calls.
#[cfg(target_os = "android")]
static NATIVE_BRIDGE_CLASS: std::sync::OnceLock<GlobalRef> = std::sync::OnceLock::new();

/// Store the JavaVM reference and cache the NativeBridge class from a
/// JNI call on the main thread.  Must be called with `&mut env` so we
/// can call `find_class` + `new_global_ref`.
#[cfg(target_os = "android")]
fn store_jvm(env: &mut JNIEnv) {
    if JAVA_VM.get().is_none() {
        match env.get_java_vm() {
            Ok(vm) => {
                let _ = JAVA_VM.set(vm);
                log::info!("[android_bridge] JavaVM reference stored");
            }
            Err(e) => {
                log::warn!("[android_bridge] Failed to get JavaVM: {}", e);
            }
        }
    }

    // Cache the NativeBridge class while we're on the main thread
    if NATIVE_BRIDGE_CLASS.get().is_none() {
        match env.find_class("com/ytdl/desktop/NativeBridge") {
            Ok(class) => match env.new_global_ref(&class) {
                Ok(global) => {
                    let _ = NATIVE_BRIDGE_CLASS.set(global);
                    log::info!("[android_bridge] NativeBridge class cached as GlobalRef");
                }
                Err(e) => log::warn!("[android_bridge] new_global_ref failed: {}", e),
            },
            Err(e) => log::warn!("[android_bridge] find_class(NativeBridge) failed: {}", e),
        }
    }
}

/// Obtain the NativeBridge `JClass` for use in `call_static_method`.
/// Uses the cached `GlobalRef` (works from any thread), falling back to
/// `find_class` if the cache was not populated yet.
#[cfg(target_os = "android")]
fn get_native_bridge_class<'a>(env: &mut JNIEnv<'a>) -> Result<JClass<'a>, String> {
    if let Some(global) = NATIVE_BRIDGE_CLASS.get() {
        // Create a local reference from the cached global ref.
        // The local ref is tied to the current JNI frame's lifetime.
        let local: JObject<'a> = env
            .new_local_ref(global.as_obj())
            .map_err(|e| format!("Failed to create local ref from cached NativeBridge: {}", e))?;
        // Safety: The GlobalRef was created from a JClass (java.lang.Class instance).
        Ok(unsafe { JClass::from_raw(local.into_raw()) })
    } else {
        // Fallback — only works from the main thread / JVM-started threads
        log::warn!("[android_bridge] NativeBridge GlobalRef not cached — trying find_class");
        env.find_class("com/ytdl/desktop/NativeBridge")
            .map_err(|e| format!("NativeBridge class not found (uncached): {}", e))
    }
}

/// Returns (is_installed, has_run_command_permission)
#[cfg(target_os = "android")]
pub fn termux_info() -> (bool, bool) {
    use std::sync::atomic::Ordering;
    (
        TERMUX_INSTALLED.load(Ordering::Relaxed),
        TERMUX_HAS_PERMISSION.load(Ordering::Relaxed),
    )
}

/// Returns the app cache directory set by Kotlin, if available
#[cfg(target_os = "android")]
pub fn get_app_cache_dir() -> Option<&'static std::path::PathBuf> {
    APP_CACHE_DIR.get()
}

// ══════════════════════════════════════════════════════════════════════════════
// Rust → Kotlin bridge: call NativeBridge static methods via JNI
// ══════════════════════════════════════════════════════════════════════════════

/// Run a shell command in Termux background, redirecting output to `output_path`.
/// Returns true if the intent was sent successfully.
#[cfg(target_os = "android")]
pub fn run_termux_check(command: &str, output_path: &str) -> Result<bool, String> {
    let vm = JAVA_VM.get().ok_or("JavaVM not initialized — JNI bridge unavailable")?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach JVM thread: {}", e))?;

    // Clear any pending JNI exceptions from previous calls
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    let class = get_native_bridge_class(&mut env)?;

    let j_command = env.new_string(command)
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;
    let j_output = env.new_string(output_path)
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;

    let result = env.call_static_method(
        class,
        "runTermuxCheck",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&j_command), JValue::Object(&j_output)],
    );

    // Check for JVM exceptions after the call
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JVM exception during runTermuxCheck".to_string());
    }

    result
        .map_err(|e| format!("JNI call runTermuxCheck failed: {}", e))?
        .z()
        .map_err(|e| format!("Invalid JNI return type: {}", e))
}

/// Launch a yt-dlp download in Termux foreground terminal.
/// The user sees the download progress in Termux.
/// `download_id` is written to a sentinel file so the Rust poller can detect completion.
#[cfg(target_os = "android")]
pub fn run_termux_download(url: &str, output_dir: &str, format_id: &str, extra_args: &[String], download_id: &str) -> Result<bool, String> {
    let vm = JAVA_VM.get().ok_or("JavaVM not initialized — JNI bridge unavailable")?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach JVM thread: {}", e))?;

    // Clear any pending JNI exceptions
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    let class = get_native_bridge_class(&mut env)?;

    let j_url = env.new_string(url)
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;
    let j_output = env.new_string(output_dir)
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;
    let j_format = env.new_string(format_id)
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;
    // Serialize extra args with ||| separator (avoids comma issues)
    let j_extras = env.new_string(extra_args.join("|||"))
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;
    let j_download_id = env.new_string(download_id)
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;

    let result = env.call_static_method(
        class,
        "runTermuxDownload",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&j_url),
            JValue::Object(&j_output),
            JValue::Object(&j_format),
            JValue::Object(&j_extras),
            JValue::Object(&j_download_id),
        ],
    ).map_err(|e| format!("JNI call runTermuxDownload failed: {}", e));

    // Check for JVM exceptions after the call
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JVM exception during runTermuxDownload".to_string());
    }

    result?
        .z()
        .map_err(|e| format!("Invalid JNI return type: {}", e))
}

/// Open Termux and run the yt-dlp/ffmpeg install commands.
#[cfg(target_os = "android")]
pub fn launch_termux_setup() -> Result<bool, String> {
    let vm = JAVA_VM.get().ok_or("JavaVM not initialized — JNI bridge unavailable")?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach JVM thread: {}", e))?;

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    let class = get_native_bridge_class(&mut env)?;

    let result = env.call_static_method(
        class,
        "launchTermuxSetup",
        "()Z",
        &[],
    );

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JVM exception during launchTermuxSetup".to_string());
    }

    result
        .map_err(|e| format!("JNI call launchTermuxSetup failed: {}", e))?
        .z()
        .map_err(|e| format!("Invalid JNI return type: {}", e))
}

/// Open the Termux app.
#[cfg(target_os = "android")]
pub fn open_termux_app() -> Result<bool, String> {
    let vm = JAVA_VM.get().ok_or("JavaVM not initialized — JNI bridge unavailable")?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach JVM thread: {}", e))?;

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    let class = get_native_bridge_class(&mut env)?;

    let result = env.call_static_method(
        class,
        "openTermuxApp",
        "()Z",
        &[],
    );

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JVM exception during openTermuxApp".to_string());
    }

    result
        .map_err(|e| format!("JNI call openTermuxApp failed: {}", e))?
        .z()
        .map_err(|e| format!("Invalid JNI return type: {}", e))
}

// ══════════════════════════════════════════════════════════════════════════════
// Rust → Kotlin bridge: storage permission
// ══════════════════════════════════════════════════════════════════════════════

/// Check if the app has MANAGE_EXTERNAL_STORAGE (Android 11+) or
/// WRITE_EXTERNAL_STORAGE (Android 10-). Required for reading/writing
/// files in shared storage exchanged with Termux.
#[cfg(target_os = "android")]
pub fn check_storage_permission() -> Result<bool, String> {
    let vm = JAVA_VM.get().ok_or("JavaVM not initialized — JNI bridge unavailable")?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach JVM thread: {}", e))?;

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    let class = get_native_bridge_class(&mut env)?;

    let result = env.call_static_method(
        class,
        "checkStoragePermission",
        "()Z",
        &[],
    );

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JVM exception during checkStoragePermission".to_string());
    }

    result
        .map_err(|e| format!("JNI call checkStoragePermission failed: {}", e))?
        .z()
        .map_err(|e| format!("Invalid JNI return type: {}", e))
}

/// Open system Settings to let the user grant MANAGE_EXTERNAL_STORAGE.
#[cfg(target_os = "android")]
pub fn request_storage_permission() -> Result<bool, String> {
    let vm = JAVA_VM.get().ok_or("JavaVM not initialized — JNI bridge unavailable")?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach JVM thread: {}", e))?;

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    let class = get_native_bridge_class(&mut env)?;

    let result = env.call_static_method(
        class,
        "requestStoragePermission",
        "()Z",
        &[],
    );

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JVM exception during requestStoragePermission".to_string());
    }

    result
        .map_err(|e| format!("JNI call requestStoragePermission failed: {}", e))?
        .z()
        .map_err(|e| format!("Invalid JNI return type: {}", e))
}

/// Open a URL in the system browser via Android's Intent.ACTION_VIEW.
/// Used by the `open_external` command on Android where `open::that()` doesn't work.
#[cfg(target_os = "android")]
pub fn open_url(url: &str) -> Result<bool, String> {
    let vm = JAVA_VM.get().ok_or("JavaVM not initialized — JNI bridge unavailable")?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach JVM thread: {}", e))?;

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    let class = get_native_bridge_class(&mut env)?;

    let j_url = env.new_string(url)
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;

    let result = env.call_static_method(
        class,
        "openUrl",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&j_url)],
    );

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JVM exception during openUrl".to_string());
    }

    result
        .map_err(|e| format!("JNI call openUrl failed: {}", e))?
        .z()
        .map_err(|e| format!("Invalid JNI return type: {}", e))
}

/// Open a local file or directory on Android via Intent.
/// Used by the `open_path` command on Android where `open::that()` doesn't work.
#[cfg(target_os = "android")]
pub fn open_file_path(path: &str) -> Result<bool, String> {
    let vm = JAVA_VM.get().ok_or("JavaVM not initialized — JNI bridge unavailable")?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach JVM thread: {}", e))?;

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    let class = get_native_bridge_class(&mut env)?;

    let j_path = env.new_string(path)
        .map_err(|e| format!("Failed to create JNI string: {}", e))?;

    let result = env.call_static_method(
        class,
        "openFilePath",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&j_path)],
    );

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JVM exception during openFilePath".to_string());
    }

    result
        .map_err(|e| format!("JNI call openFilePath failed: {}", e))?
        .z()
        .map_err(|e| format!("Invalid JNI return type: {}", e))
}

// ── JNI: nativeSetNativeLibDir ─────────────────────────────────────────────

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_ytdl_desktop_NativeBridge_nativeSetNativeLibDir(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jboolean {
    store_jvm(&mut env);
    let native_path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(e) => {
            log::warn!("[android_bridge] Failed to read JNI path string: {}", e);
            return JNI_FALSE;
        }
    };

    if native_path.trim().is_empty() {
        log::warn!("[android_bridge] Received empty native lib path from Kotlin");
        return JNI_FALSE;
    }

    crate::download::set_native_lib_dir_override(native_path);
    JNI_TRUE
}

// ── JNI: nativeSetCacheDir ─────────────────────────────────────────────────

/// Called by Kotlin to set the app's cache directory (avoids unreliable /proc parsing)
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_ytdl_desktop_NativeBridge_nativeSetCacheDir(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jboolean {
    store_jvm(&mut env);
    let cache_path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(e) => {
            log::warn!("[android_bridge] Failed to read cache dir string: {}", e);
            return JNI_FALSE;
        }
    };

    let trimmed = cache_path.trim();
    if trimmed.is_empty() {
        return JNI_FALSE;
    }

    let path = std::path::PathBuf::from(trimmed);
    let _ = std::fs::create_dir_all(&path);
    let _ = APP_CACHE_DIR.set(path);
    log::info!("[android_bridge] App cache dir set via JNI: {}", trimmed);
    JNI_TRUE
}

// ── JNI: nativeSetTermuxInfo ───────────────────────────────────────────────

/// Called by Kotlin `NativeBridge.setTermuxInfo()` (from `TermuxBridge.init()`)
/// immediately after the app starts.  Stores Termux availability flags so that
/// Rust command handlers can decide whether to delegate work to Termux via
/// Android Intents.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_ytdl_desktop_NativeBridge_nativeSetTermuxInfo(
    mut env: JNIEnv,
    _class: JClass,
    installed: jboolean,
    has_permission: jboolean,
) {
    store_jvm(&mut env);
    use std::sync::atomic::Ordering;
    let inst = installed == JNI_TRUE;
    let perm = has_permission == JNI_TRUE;
    TERMUX_INSTALLED.store(inst, Ordering::Relaxed);
    TERMUX_HAS_PERMISSION.store(perm, Ordering::Relaxed);
    log::info!(
        "[android_bridge] Termux info received: installed={} has_run_permission={}",
        inst,
        perm
    );
}
