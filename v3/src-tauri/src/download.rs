use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::Manager;
use tokio::process::Command;

use crate::error::{AppError, AppResult};

/// On Android, returns the shared storage base directory for files accessible by
/// both our app and Termux.  Prefers the `EXTERNAL_STORAGE` environment variable
/// (set by Android runtime), falling back to `/sdcard` which is a stable symlink
/// on all supported Android versions.
#[cfg(target_os = "android")]
pub fn android_shared_download_dir() -> String {
    let base = std::env::var("EXTERNAL_STORAGE").unwrap_or_else(|_| "/sdcard".to_string());
    format!("{}/Download/YTDL", base)
}

/// Directory used for short-lived check output files (yt-dlp version probes, etc.)
/// shared between our app and Termux on Android.
#[cfg(target_os = "android")]
pub fn android_shared_checks_dir() -> String {
    let base = android_shared_download_dir();
    let dir = format!("{}/.checks", base);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Directory for Termux download completion sentinel files.
/// After yt-dlp finishes, the shell command writes exit status here so the
/// Rust poller can detect completion and update the DB/UI.
#[cfg(target_os = "android")]
pub fn android_shared_status_dir() -> String {
    let base = android_shared_download_dir();
    let dir = format!("{}/.status", base);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Cached result of nativeLibraryDir detection (computed once per process lifetime).
#[cfg(target_os = "android")]
static NATIVE_LIB_DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
#[cfg(target_os = "android")]
static NATIVE_LIB_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_native_lib_dir_override(path: String) {
    let candidate = PathBuf::from(path.trim());
    if candidate.as_os_str().is_empty() {
        return;
    }

    if candidate.exists() {
        let _ = NATIVE_LIB_DIR_OVERRIDE.set(candidate.clone());
        log::info!(
            "[set_native_lib_dir_override] Android native lib dir set via JNI: {}",
            candidate.display()
        );
    } else {
        log::warn!(
            "[set_native_lib_dir_override] JNI provided non-existing path: {}",
            candidate.display()
        );
    }
}

/// On Android, find the nativeLibraryDir where APK jniLibs are extracted.
/// This directory has execute permissions (unlike app_data_dir which is noexec).
/// Detection uses multiple strategies in order of reliability.
#[cfg(target_os = "android")]
pub fn get_native_lib_dir() -> Option<PathBuf> {
    if let Some(path) = NATIVE_LIB_DIR_OVERRIDE.get() {
        return Some(path.clone());
    }

    NATIVE_LIB_DIR.get_or_init(get_native_lib_dir_impl).clone()
}

#[cfg(target_os = "android")]
fn get_native_lib_dir_impl() -> Option<PathBuf> {
    use std::io::BufRead;

    // Known .so file names to look for
    let known_libs = [
        "libytdl_lib.so", "libc++_shared.so",
        "libytdlp.so", "libffmpeg.so", "libffprobe.so",
    ];

    // 0) MOST RELIABLE: read file written by MainActivity.kt on startup.
    //    MainActivity writes applicationInfo.nativeLibraryDir to files/native_lib_dir.txt
    let pkg = "com.ytdl.desktop";
    let cache_file = PathBuf::from(format!("/data/data/{}/files/native_lib_dir.txt", pkg));
    if let Ok(content) = std::fs::read_to_string(&cache_file) {
        let dir = PathBuf::from(content.trim());
        if dir.exists() {
            log::info!("[get_native_lib_dir] Found via MainActivity file: {}", dir.display());
            return Some(dir);
        }
    }

    // 1) LD_LIBRARY_PATH env var (not always set in process env, but cheap to check)
    if let Ok(ld_path) = std::env::var("LD_LIBRARY_PATH") {
        for part in ld_path.split(':') {
            if part.is_empty() { continue; }
            let path = PathBuf::from(part);
            for lib in &known_libs {
                if path.join(lib).exists() {
                    log::info!("[get_native_lib_dir] Found via LD_LIBRARY_PATH: {}", path.display());
                    return Some(path);
                }
            }
        }
    }

    // 2) /proc/self/maps — reliable since it shows all mapped .so files
    'maps: {
        let Ok(maps_file) = std::fs::File::open("/proc/self/maps") else { break 'maps; };
        let reader = std::io::BufReader::new(maps_file);

        for line in reader.lines().flatten() {
            let is_known = known_libs.iter().any(|lib| line.contains(lib));
            if !is_known || !line.contains("/data/app/") {
                continue;
            }

            let Some(path_start) = line.find('/') else { continue };
            let mut full_path = line[path_start..].trim().to_string();

            // Handle APK-in-zip: /path/base.apk!/lib/arm64-v8a/lib.so
            // With extractNativeLibs="true", the extracted dir is next to the APK
            if let Some((apk_path, inner)) = full_path.split_once('!') {
                let apk_parent = std::path::Path::new(apk_path).parent()
                    .unwrap_or(std::path::Path::new("/"));
                let arch = if inner.contains("arm64-v8a") { "arm64-v8a" } else { "arm64" };
                let candidate = apk_parent.join("lib").join(arch);
                if candidate.exists() {
                    log::info!("[get_native_lib_dir] Resolved apk-in-zip → extracted lib dir: {}", candidate.display());
                    return Some(candidate);
                }
                continue;
            }

            // Strip " (deleted)" suffix
            if let Some(stripped) = full_path.strip_suffix(" (deleted)") {
                full_path = stripped.to_string();
            }

            if let Some(parent) = std::path::Path::new(&full_path).parent() {
                let dir = parent.to_path_buf();
                if dir.exists() {
                    log::info!("[get_native_lib_dir] Found via /proc/self/maps: {}", dir.display());
                    return Some(dir);
                }
            }
        }
    }

    // 3) /proc/self/map_files/ — symlinks to all mapped files
    if let Ok(fds) = std::fs::read_dir("/proc/self/map_files") {
        for fd in fds.flatten() {
            if let Ok(target) = std::fs::read_link(fd.path()) {
                let target_str = target.to_string_lossy();
                if known_libs.iter().any(|lib| target_str.ends_with(lib))
                    && target_str.contains("/data/app/")
                {
                    if let Some(parent) = target.parent() {
                        if parent.exists() {
                            log::info!("[get_native_lib_dir] Found via /proc/self/map_files: {}", parent.display());
                            return Some(parent.to_path_buf());
                        }
                    }
                }
            }
        }
    }

    // 4) Direct package scan in /data/app/
    let data_app = PathBuf::from("/data/app");
    if let Ok(level1) = std::fs::read_dir(&data_app) {
        for entry1 in level1.flatten() {
            let top = entry1.path();
            if let Ok(level2) = std::fs::read_dir(&top) {
                for entry2 in level2.flatten() {
                    let app_dir = entry2.path();
                    let app_name = app_dir.file_name().unwrap_or_default().to_string_lossy();
                    if !app_name.starts_with(pkg) {
                        continue;
                    }
                    for arch in &["lib/arm64-v8a", "lib/arm64"] {
                        let candidate = app_dir.join(arch);
                        if known_libs.iter().any(|lib| candidate.join(lib).exists()) {
                            log::info!("[get_native_lib_dir] Found by package scan: {}", candidate.display());
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }

    log::warn!("[get_native_lib_dir] Could not determine nativeLibraryDir");
    None
}

/// Resolves the binary directory for storing yt-dlp, ffmpeg, and whisper binaries.
/// Uses app_data_dir on all platforms to ensure a user-writable location.
/// On Linux desktop, resource_dir typically points to read-only installation directories
/// (e.g., /usr/lib/ytdl/ or /opt/ytdl/), which causes OS Error 13 (Permission Denied).
/// On Android, uses app_data_dir which is always writable.
pub fn get_binary_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    // Try app_data_dir first (writable on all platforms including Android)
    if let Ok(base_dir) = app_handle.path().app_data_dir() {
        let bin_dir = base_dir.join("binaries");
        log::info!("[get_binary_dir] Using app_data_dir: {}", bin_dir.display());
        return bin_dir;
    }

    // Fallback: try cache_dir
    if let Ok(base_dir) = app_handle.path().cache_dir() {
        let bin_dir = base_dir.join("binaries");
        log::warn!("[get_binary_dir] app_data_dir failed, using cache_dir: {}", bin_dir.display());
        return bin_dir;
    }

    // Last fallback: temp directory
    if let Ok(base_dir) = app_handle.path().temp_dir() {
        let bin_dir = base_dir.join("binaries");
        log::warn!("[get_binary_dir] cache_dir failed, using temp_dir: {}", bin_dir.display());
        return bin_dir;
    }

    // Absolute last resort - use current directory
    let bin_dir = PathBuf::from("binaries");
    log::error!("[get_binary_dir] All directories failed, using current dir: {}", bin_dir.display());
    bin_dir
}

/// Create a Command that hides the console window on Windows
#[cfg(windows)]
pub fn create_hidden_command(program: &str) -> Command {
    #[allow(unused_imports)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
pub fn create_hidden_command(program: &str) -> Command {
    let mut cmd = Command::new(program);

    // On Android, configure the process environment so that:
    //  1. PyInstaller-bundled one-file binaries (yt-dlp musl-static) can extract to TMPDIR
    //  2. PATH includes nativeLibraryDir and system bin directories
    //  3. LD_LIBRARY_PATH includes native libs directory
    //  4. Python and SSL env vars are set for Python-based yt-dlp
    #[cfg(target_os = "android")]
    {
        // ── Writable temp directory (needed by PyInstaller --onefile) ────────
        // Use the app's cache dir, NOT the system /tmp (which doesn't exist on Android)
        let cache_dir = get_android_cache_dir();
        if let Some(ref cache) = cache_dir {
            let tmpdir = cache.join("pytmp");
            let _ = std::fs::create_dir_all(&tmpdir);
            cmd.env("TMPDIR", tmpdir.to_string_lossy().to_string());

            // HOME: Python stores user-site packages under ~/.local
            // Point it to app data dir so pip --user works
            if let Some(app_data) = cache.parent() {
                cmd.env("HOME", app_data.to_string_lossy().to_string());

                // Python user-site path (for pip-installed yt-dlp, cookiecutter, etc.)
                let user_site = app_data.join("python_user_site");
                let _ = std::fs::create_dir_all(&user_site);
                cmd.env("PYTHONUSERBASE", user_site.to_string_lossy().to_string());
            }
        }

        // ── PATH ─────────────────────────────────────────────────────────────
        let mut path_dirs: Vec<String> = Vec::new();
        // nativeLibraryDir first (exec permission, contains libytdlp.so etc.)
        if let Some(native_dir) = get_native_lib_dir() {
            path_dirs.push(native_dir.to_string_lossy().to_string());
        }
        // System bins (always present on Android)
        path_dirs.push("/system/bin".to_string());
        path_dirs.push("/system/xbin".to_string());
        // Note: Termux bins are in a separate SELinux domain and CANNOT be
        // executed from this app's context on non-rooted Android.  We leave
        // them in PATH for informational purposes but execution will fail due
        // to SELinux type-enforcement.  The TermuxBridge (Intent) is the
        // correct way to delegate work to Termux.
        path_dirs.push("/data/data/com.termux/files/usr/bin".to_string());
        if let Ok(existing) = std::env::var("PATH") {
            path_dirs.push(existing);
        }
        cmd.env("PATH", path_dirs.join(":"));

        // ── LD_LIBRARY_PATH ──────────────────────────────────────────────────
        let mut ld_dirs: Vec<String> = Vec::new();
        if let Some(native_dir) = get_native_lib_dir() {
            ld_dirs.push(native_dir.to_string_lossy().to_string());
        }
        // System libs
        ld_dirs.push("/system/lib64".to_string());
        ld_dirs.push("/system/lib".to_string());
        if let Ok(existing) = std::env::var("LD_LIBRARY_PATH") {
            ld_dirs.push(existing);
        }
        if !ld_dirs.is_empty() {
            cmd.env("LD_LIBRARY_PATH", ld_dirs.join(":"));
        }

        // ── SSL / Python TLS ─────────────────────────────────────────────────
        // Android system CA bundle (used by statically-compiled Python in yt-dlp)
        let android_ca_certs = [
            "/system/etc/security/cacerts",     // Android system CA directory
            "/data/data/com.termux/files/usr/etc/tls/cert.pem",  // Termux CA bundle
            "/data/data/com.termux/files/usr/lib/python3.12/site-packages/certifi/cacert.pem",
        ];
        // SSL_CERT_FILE: single-file CA bundle used by Python's ssl module
        for ca in &android_ca_certs {
            let p = std::path::Path::new(ca);
            if p.is_file() {
                cmd.env("SSL_CERT_FILE", ca);
                break;
            }
        }
        // SSL_CERT_DIR: directory of CA certs (used when SSL_CERT_FILE isn't set)
        if std::path::Path::new("/system/etc/security/cacerts").is_dir() {
            cmd.env("SSL_CERT_DIR", "/system/etc/security/cacerts");
        }

        // ── Python for yt-dlp module approach ───────────────────────────────
        // If yt-dlp is running as "python3 -m yt_dlp" from Termux Python,
        // PYTHONPATH must include the Termux site-packages
        if let Some(ref cache) = cache_dir {
            if let Some(app_data) = cache.parent() {
                let user_site = app_data.join("python_user_site");
                if let Ok(existing_pp) = std::env::var("PYTHONPATH") {
                    cmd.env(
                        "PYTHONPATH",
                        format!("{}:{}", user_site.display(), existing_pp),
                    );
                } else {
                    cmd.env("PYTHONPATH", user_site.to_string_lossy().to_string());
                }
            }
        }

        // ── Text/locale settings ─────────────────────────────────────────────
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("LC_ALL", "en_US.UTF-8");
    }

    cmd
}

/// Get the app's cache directory on Android.
/// Prefers JNI-provided path from Kotlin, falls back to heuristics.
#[cfg(target_os = "android")]
fn get_android_cache_dir() -> Option<PathBuf> {
    // 1) Best: use the cache dir set by Kotlin via JNI (most reliable)
    if let Some(cache_dir) = crate::android_bridge::get_app_cache_dir() {
        if cache_dir.exists() || std::fs::create_dir_all(cache_dir).is_ok() {
            return Some(cache_dir.clone());
        }
    }

    // 2) Try TMPDIR environment variable
    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        let path = PathBuf::from(&tmpdir);
        if path.exists() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path);
        }
    }

    // 3) Try to find from /proc/self/cmdline (contains package name)
    if let Ok(cmdline) = std::fs::read_to_string("/proc/self/cmdline") {
        let pkg = cmdline.trim_matches('\0').split('\0').next().unwrap_or("");
        if !pkg.is_empty() {
            let cache = PathBuf::from(format!("/data/data/{}/cache", pkg));
            if cache.exists() || std::fs::create_dir_all(&cache).is_ok() {
                return Some(cache);
            }
        }
    }

    // 4) Fallback for our known package
    let fallback = PathBuf::from("/data/data/com.ytdl.desktop/cache");
    if fallback.exists() || std::fs::create_dir_all(&fallback).is_ok() {
        return Some(fallback);
    }

    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub thumbnail: String,
    pub duration: f64,
    pub uploader: String,
    pub url: String,
    pub formats: Vec<VideoFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub filesize: Option<i64>,
    pub vcodec: String,
    pub acodec: String,
    pub fps: Option<f64>,
    pub tbr: Option<f64>,
    pub format_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub id: String,
    pub title: String,
    pub url: String,
    pub index: usize,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    pub id: String,
    pub title: String,
    pub entries: Vec<PlaylistEntry>,
    pub entry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    pub progress: f64,
    pub speed: String,
    pub eta: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct ActiveDownload {
    pub id: String,
    pub url: String,
    pub status: String,
    pub cancel_token: tokio::sync::watch::Sender<bool>,
}

pub struct DownloadManager {
    pub active: HashMap<String, ActiveDownload>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    pub fn pause(&mut self, id: &str) -> bool {
        if let Some(download) = self.active.get_mut(id) {
            if download.status == "downloading" {
                download.status = "paused".to_string();
                return true;
            }
        }
        false
    }

    pub fn resume(&mut self, id: &str) -> bool {
        if let Some(download) = self.active.get_mut(id) {
            if download.status == "paused" {
                download.status = "downloading".to_string();
                return true;
            }
        }
        false
    }

    pub fn cancel(&mut self, id: &str) {
        if let Some(download) = self.active.get(id) {
            let _ = download.cancel_token.send(true);
        }
        self.active.remove(id);
    }

    /// Get count of currently active downloads
    pub fn get_active_count(&self) -> usize {
        self.active.iter()
            .filter(|(_, d)| d.status == "downloading")
            .count()
    }

    /// Check if we can start a new download based on the concurrent limit
    pub fn can_start_download(&self, max_concurrent: usize) -> bool {
        self.get_active_count() < max_concurrent
    }

    /// Get queued downloads (downloads waiting to start)
    pub fn get_queued_ids(&self) -> Vec<String> {
        self.active.iter()
            .filter(|(_, d)| d.status == "queued")
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// Resolves the yt-dlp binary path.
/// Priority:
/// 1) Environment variable YTDLP_PATH
/// 2) [Android] Bundled libytdlp.so in nativeLibraryDir
/// 3) [Android] Termux yt-dlp installation
/// 4) Installed binary in bin_dir
/// 5) System PATH
pub fn get_ytdlp_path(app_handle: &tauri::AppHandle) -> String {
    // Check environment variable first (for custom path)
    if let Ok(custom_path) = std::env::var("YTDLP_PATH") {
        if !custom_path.is_empty() && PathBuf::from(&custom_path).exists() {
            log::info!("[get_ytdlp_path] Using custom path from YTDLP_PATH: {}", custom_path);
            return custom_path;
        }
    }
    
    // On Android, check nativeLibraryDir first (has exec permissions for .so files).
    // NOTE: Bundled Linux ARM64 `yt-dlp` binaries will likely fail on Android because
    // they reference a Linux ELF interpreter (/lib/ld-linux-aarch64.so.1 or musl equiv.)
    // that does NOT exist on Android (uses /system/bin/linker64). The kernel returns
    // ENOENT ("No such file or directory") even though the file itself exists.
    // The working approach is Termux RUN_COMMAND — see commands.rs.
    #[cfg(target_os = "android")]
    {
        if let Some(native_dir) = get_native_lib_dir() {
            let native_ytdlp = native_dir.join("libytdlp.so");
            if native_ytdlp.exists() {
                log::info!("[get_ytdlp_path] Found bundled yt-dlp in nativeLibraryDir: {}", native_ytdlp.display());
                return native_ytdlp.to_string_lossy().to_string();
            }
        }
        // NOTE: Termux direct path checks (/data/data/com.termux/...) are unreachable
        // from our app's SELinux domain (untrusted_app). Even if yt-dlp is installed
        // in Termux, PathBuf::exists() always returns false due to SELinux denials.
        // Use android_bridge::run_termux_check() via RUN_COMMAND Intent instead.
    }
    
    let bin_name: &str = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
    let bin_dir = get_binary_dir(app_handle);
    let sidecar: PathBuf = bin_dir.join(bin_name);
    
    log::info!("[get_ytdlp_path] Looking for yt-dlp at: {}", sidecar.display());
    
    if sidecar.exists() {
        log::info!("[get_ytdlp_path] Found yt-dlp at: {}", sidecar.display());
        return sidecar.to_string_lossy().to_string();
    }
    
    log::warn!("[get_ytdlp_path] yt-dlp not found at {}, falling back to PATH", sidecar.display());
    
    // Fallback to PATH
    let fallback = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" }.to_string();
    log::info!("[get_ytdlp_path] Using fallback: {}", fallback);
    fallback
}

/// Resolves the ffmpeg binary path.
/// Priority:
/// 1) Environment variable FFMPEG_PATH
/// 2) [Android] Bundled libffmpeg.so in nativeLibraryDir
/// 3) [Android] Termux ffmpeg installation
/// 4) Installed binary in bin_dir
/// 5) System PATH
pub fn get_ffmpeg_path(app_handle: &tauri::AppHandle) -> String {
    // Check environment variable first (for custom path)
    if let Ok(custom_path) = std::env::var("FFMPEG_PATH") {
        if !custom_path.is_empty() && PathBuf::from(&custom_path).exists() {
            log::info!("[get_ffmpeg_path] Using custom path from FFMPEG_PATH: {}", custom_path);
            return custom_path;
        }
    }
    
    // On Android, check nativeLibraryDir first (same ELF interpreter caveat as yt-dlp).
    #[cfg(target_os = "android")]
    {
        if let Some(native_dir) = get_native_lib_dir() {
            let native_ffmpeg = native_dir.join("libffmpeg.so");
            if native_ffmpeg.exists() {
                log::info!("[get_ffmpeg_path] Found bundled ffmpeg in nativeLibraryDir: {}", native_ffmpeg.display());
                return native_ffmpeg.to_string_lossy().to_string();
            }
        }
        // NOTE: Termux direct path checks are unreachable — see get_ytdlp_path() comment.
    }
    
    let bin_name: &str = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let bin_dir = get_binary_dir(app_handle);
    let sidecar: PathBuf = bin_dir.join(bin_name);
    
    log::info!("[get_ffmpeg_path] Looking for ffmpeg at: {}", sidecar.display());
    
    if sidecar.exists() {
        log::info!("[get_ffmpeg_path] Found ffmpeg at: {}", sidecar.display());
        return sidecar.to_string_lossy().to_string();
    }
    
    log::warn!("[get_ffmpeg_path] ffmpeg not found at {}, falling back to PATH", sidecar.display());
    
    // Fallback to PATH
    let fallback = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }.to_string();
    log::info!("[get_ffmpeg_path] Using fallback: {}", fallback);
    fallback
}

/// Get ffprobe path (used alongside ffmpeg)
pub fn get_ffprobe_path(app_handle: &tauri::AppHandle) -> String {
    // On Android, check nativeLibraryDir first
    #[cfg(target_os = "android")]
    {
        if let Some(native_dir) = get_native_lib_dir() {
            let native_ffprobe = native_dir.join("libffprobe.so");
            if native_ffprobe.exists() {
                return native_ffprobe.to_string_lossy().to_string();
            }
        }
        
        // Check Termux
        let termux_path = "/data/data/com.termux/files/usr/bin/ffprobe";
        if PathBuf::from(termux_path).exists() {
            return termux_path.to_string();
        }
    }
    
    let bin_name: &str = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    let bin_dir = get_binary_dir(app_handle);
    let sidecar: PathBuf = bin_dir.join(bin_name);
    
    if sidecar.exists() {
        return sidecar.to_string_lossy().to_string();
    }
    
    if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" }.to_string()
}

/// Fetch video metadata via yt-dlp --dump-json
pub async fn fetch_video_info(ytdlp: &str, url: &str) -> AppResult<VideoInfo> {
    log::info!("[fetch_video_info] Using yt-dlp: {}", ytdlp);
    log::info!("[fetch_video_info] URL: {}", url);
    
    let output = create_hidden_command(ytdlp)
        .args(["--dump-json", "--no-download", "--no-warnings", url])
        .output()
        .await
        .map_err(|e| {
            let error_code = e.raw_os_error().unwrap_or(0);
            if error_code == 13 {
                log::error!("[fetch_video_info] Permission denied (OS error 13) when running yt-dlp at {}", ytdlp);
                AppError::YtDlp(format!(
                    "Permission denied (OS error 13) when running yt-dlp.\n\
                    Please ensure yt-dlp is properly installed and has execute permissions.\n\
                    Path: {}",
                    ytdlp
                ))
            } else {
                AppError::YtDlp(format!("Failed to execute yt-dlp: {}", e))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::YtDlp(format!(
            "yt-dlp exited with code {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| AppError::YtDlp(format!("Failed to parse yt-dlp JSON: {}", e)))?;

    parse_video_info_json_inner(&json, url)
}

/// Parse a yt-dlp JSON object into a VideoInfo struct.
/// Used by both `fetch_video_info` (desktop) and the Termux-based flow (Android).
pub fn parse_video_info_json(json: &serde_json::Value) -> AppResult<VideoInfo> {
    let url = json["webpage_url"]
        .as_str()
        .or_else(|| json["original_url"].as_str())
        .unwrap_or("");
    parse_video_info_json_inner(json, url)
}

fn parse_video_info_json_inner(json: &serde_json::Value, url: &str) -> AppResult<VideoInfo> {
    let formats = json["formats"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|f| {
            Some(VideoFormat {
                format_id: f["format_id"].as_str()?.to_string(),
                ext: f["ext"].as_str().unwrap_or("unknown").to_string(),
                resolution: f["resolution"].as_str().unwrap_or("?").to_string(),
                width: f["width"].as_i64(),
                height: f["height"].as_i64(),
                filesize: f["filesize"].as_i64().or(f["filesize_approx"].as_i64()),
                vcodec: f["vcodec"].as_str().unwrap_or("none").to_string(),
                acodec: f["acodec"].as_str().unwrap_or("none").to_string(),
                fps: f["fps"].as_f64(),
                tbr: f["tbr"].as_f64(),
                format_note: f["format_note"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();

    Ok(VideoInfo {
        id: json["id"].as_str().unwrap_or("").to_string(),
        title: json["title"].as_str().unwrap_or("Unknown").to_string(),
        thumbnail: json["thumbnail"].as_str().unwrap_or("").to_string(),
        duration: json["duration"].as_f64().unwrap_or(0.0),
        uploader: json["uploader"].as_str().unwrap_or("Unknown").to_string(),
        url: url.to_string(),
        formats,
    })
}

/// Fetch playlist metadata via yt-dlp --flat-playlist
pub async fn fetch_playlist_info(ytdlp: &str, url: &str) -> AppResult<PlaylistInfo> {
    let output = create_hidden_command(ytdlp)
        .args([
            "-J",
            "--flat-playlist",
            "--no-warnings",
            url,
        ])
        .output()
        .await
        .map_err(|e| AppError::YtDlp(format!("Failed to execute yt-dlp: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::YtDlp(format!(
            "yt-dlp exited with code {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| AppError::YtDlp(format!("Failed to parse yt-dlp JSON: {}", e)))?;

    let entries = json["entries"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .enumerate()
        .filter_map(|(idx, entry)| {
            let id = entry["id"].as_str()?;
            let title = entry["title"].as_str().unwrap_or("Unknown");
            
            // Try to get URL from entry
            let entry_url = if let Some(url_str) = entry["url"].as_str() {
                url_str.to_string()
            } else if let Some(webpage_url) = entry["webpage_url"].as_str() {
                webpage_url.to_string()
            } else if let Some(ie_key) = entry["ie_key"].as_str() {
                if ie_key.contains("Youtube") {
                    format!("https://www.youtube.com/watch?v={}", id)
                } else {
                    return None;
                }
            } else {
                return None;
            };

            Some(PlaylistEntry {
                id: id.to_string(),
                title: title.to_string(),
                url: entry_url,
                index: idx + 1,
                thumbnail: entry["thumbnail"].as_str().map(String::from),
            })
        })
        .collect::<Vec<_>>();

    Ok(PlaylistInfo {
        id: json["id"].as_str().unwrap_or("").to_string(),
        title: json["title"].as_str().unwrap_or("Playlist").to_string(),
        entry_count: entries.len(),
        entries,
    })
}

/// Run yt-dlp download with progress reporting
pub async fn run_download(
    ytdlp: &str,
    ffmpeg: &str,
    url: &str,
    output_dir: &str,
    format_id: Option<&str>,
    extra_args: &[String],
    progress_tx: tokio::sync::mpsc::Sender<DownloadProgress>,
    cancel_rx: tokio::sync::watch::Receiver<bool>,
    download_id: String,
) -> AppResult<String> {
    let output_template = format!("{}/%(title)s.%(ext)s", output_dir);

    // For --ffmpeg-location: on Android with bundled .so files, create symlinks
    // from libffmpeg.so -> ffmpeg so yt-dlp can find them by standard name
    let ffmpeg_location = {
        let ffmpeg_path = std::path::Path::new(ffmpeg);
        if let Some(parent) = ffmpeg_path.parent() {
            // If ffmpeg is named libffmpeg.so (Android bundled), create symlinks
            #[cfg(unix)]
            {
                let ffmpeg_name = ffmpeg_path.file_name().unwrap_or_default().to_string_lossy();
                if ffmpeg_name == "libffmpeg.so" {
                    // Create symlink ffmpeg -> libffmpeg.so in a writable temp dir
                    if let Ok(app_cache) = std::env::var("TMPDIR") {
                        let link_dir = std::path::PathBuf::from(&app_cache).join("ffmpeg_links");
                        let _ = std::fs::create_dir_all(&link_dir);
                        let ffmpeg_link = link_dir.join("ffmpeg");
                        let ffprobe_link = link_dir.join("ffprobe");
                        let _ = std::fs::remove_file(&ffmpeg_link);
                        let _ = std::fs::remove_file(&ffprobe_link);
                        let _ = std::os::unix::fs::symlink(ffmpeg_path, &ffmpeg_link);
                        // Also link ffprobe
                        let ffprobe_so = parent.join("libffprobe.so");
                        if ffprobe_so.exists() {
                            let _ = std::os::unix::fs::symlink(&ffprobe_so, &ffprobe_link);
                        }
                        link_dir.to_string_lossy().to_string()
                    } else {
                        parent.to_string_lossy().to_string()
                    }
                } else {
                    parent.to_string_lossy().to_string()
                }
            }
            #[cfg(not(unix))]
            {
                parent.to_string_lossy().to_string()
            }
        } else {
            ffmpeg.to_string()
        }
    };

    let mut args = vec![
        "--newline".to_string(),
        "--progress".to_string(),
        "--no-warnings".to_string(),
        "--ffmpeg-location".to_string(),
        ffmpeg_location,
        "-o".to_string(),
        output_template.clone(),
        "--print".to_string(),
        "after_move:filepath".to_string(),
    ];

    // Enable partial download resume (yt-dlp supports continuing partial files)
    args.push("--continue".to_string());

    if let Some(fid) = format_id {
        if fid == "best" {
            args.push("-f".to_string());
            args.push("bestvideo+bestaudio/best".to_string());
        } else {
            args.push("-f".to_string());
            args.push(fid.to_string());
        }
    } else {
        args.push("-f".to_string());
        args.push("bestvideo+bestaudio/best".to_string());
    }

    // Merge audio+video when separate streams
    args.push("--merge-output-format".to_string());
    args.push("mp4".to_string());

    for extra in extra_args {
        args.push(extra.clone());
    }
    args.push(url.to_string());

    log::info!("[run_download] Starting yt-dlp: {} {}", ytdlp, args.join(" "));
    log::info!("[run_download] Output dir: {}", output_dir);

    let mut child = create_hidden_command(ytdlp)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            let error_code = e.raw_os_error().unwrap_or(0);
            if error_code == 13 {
                log::error!("[run_download] Permission denied (OS error 13) when running yt-dlp at {}", ytdlp);
                AppError::Download(format!(
                    "Permission denied (OS error 13).\n\
                    yt-dlp path: {}\n\
                    Please check if yt-dlp is properly installed and has execute permissions.",
                    ytdlp
                ))
            } else {
                AppError::Download(format!("Failed to spawn yt-dlp: {}", e))
            }
        })?;

    let stdout = child.stdout.take()
        .ok_or_else(|| AppError::Download("Failed to capture yt-dlp stdout".to_string()))?;
    let id = download_id.clone();

    // Capture output file path from stdout
    let output_path = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
    let output_path_clone = output_path.clone();

    // Read stdout for progress
    let progress_handle = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(progress) = parse_ytdlp_progress(&line) {
                let _ = progress_tx
                    .send(DownloadProgress {
                        id: id.clone(),
                        progress: progress.0,
                        speed: progress.1,
                        eta: progress.2,
                        status: "downloading".to_string(),
                    })
                    .await;
            }
            if let Some(found_path) = extract_output_file_path_from_line(&line) {
                let mut path = output_path_clone.lock().await;
                *path = found_path;
            }
        }
    });

    // Wait for completion or cancellation
    tokio::select! {
        result = child.wait() => {
            progress_handle.abort();
            match result {
                Ok(status) if status.success() => {
                    let file_path = output_path.lock().await.clone();
                    if file_path.is_empty() {
                        Ok(download_id)
                    } else {
                        Ok(file_path)
                    }
                }
                Ok(status) => {
                    Err(AppError::Download(format!("yt-dlp exited with code: {}", status)))
                }
                Err(e) => Err(AppError::Download(format!("yt-dlp process error: {}", e))),
            }
        }
        _ = wait_for_cancel(cancel_rx) => {
            let _ = child.kill().await;
            Err(AppError::Download("Download cancelled".to_string()))
        }
    }
}

async fn wait_for_cancel(mut rx: tokio::sync::watch::Receiver<bool>) {
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            // Channel closed, just wait forever
            std::future::pending::<()>().await;
        }
    }
}

fn extract_output_file_path_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    if !trimmed.starts_with('[')
        && std::path::Path::new(trimmed).extension().is_some()
    {
        return Some(trimmed.to_string());
    }

    if let Some(rest) = trimmed.strip_prefix("[download] Destination: ") {
        let candidate = rest.trim();
        if std::path::Path::new(candidate).extension().is_some() {
            return Some(candidate.to_string());
        }
    }

    if let Some(rest) = trimmed.strip_prefix("[download] ") {
        if let Some(path_part) = rest.strip_suffix(" has already been downloaded") {
            let candidate = path_part.trim();
            if std::path::Path::new(candidate).extension().is_some() {
                return Some(candidate.to_string());
            }
        }
    }

    if let Some(rest) = trimmed.strip_prefix("[Merger] Merging formats into ") {
        let candidate = rest
            .trim()
            .trim_matches('"')
            .to_string();
        if std::path::Path::new(&candidate).extension().is_some() {
            return Some(candidate);
        }
    }

    None
}

/// Parse yt-dlp progress line like "[download]  50.0% of ~100MiB at 5.00MiB/s ETA 00:10"
fn parse_ytdlp_progress(line: &str) -> Option<(f64, String, String)> {
    use std::sync::OnceLock;

    static RE_PROGRESS: OnceLock<regex::Regex> = OnceLock::new();
    static RE_SPEED: OnceLock<regex::Regex> = OnceLock::new();
    static RE_ETA: OnceLock<regex::Regex> = OnceLock::new();

    if !line.contains("[download]") || !line.contains('%') {
        return None;
    }

    let re_progress = RE_PROGRESS.get_or_init(|| regex::Regex::new(r"(\d+\.?\d*)%").unwrap());
    let re_speed = RE_SPEED.get_or_init(|| regex::Regex::new(r"at\s+(\S+)").unwrap());
    let re_eta = RE_ETA.get_or_init(|| regex::Regex::new(r"ETA\s+(\S+)").unwrap());

    let progress = {
        let cap = re_progress.captures(line)?;
        cap.get(1)?.as_str().parse::<f64>().ok()?
    };

    let speed = re_speed
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

    let eta = re_eta
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

    Some((progress, speed, eta))
}
