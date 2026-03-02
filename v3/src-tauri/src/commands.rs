use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Database;
use crate::download::{self, DownloadManager, DownloadProgress};
use crate::rss;

const RSS_SYNC_BATCH_SIZE: usize = 200;

/// Validates a URL for security (SSRF protection).
/// Resolves the hostname and checks against RFC 1918, loopback, and link-local ranges
/// to prevent bypasses via decimal IPs, IPv6-mapped addresses, or DNS rebinding.
pub(crate) fn validate_url(url: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    if url.len() < 10 {
        return Err("URL is too short".to_string());
    }

    let trimmed = url.trim().to_lowercase();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }

    // Parse the URL properly
    let parsed = url::Url::parse(url.trim()).map_err(|e| format!("Invalid URL: {}", e))?;
    let host_str = parsed.host_str().ok_or("URL has no host")?;

    // Direct pattern checks for common bypass strings
    let blocked_patterns = ["localhost", "file://"];
    for pattern in blocked_patterns {
        if trimmed.contains(pattern) {
            return Err(format!("URL contains blocked pattern: {}", pattern));
        }
    }

    // Resolve hostname to IP addresses and validate each one
    let port = parsed.port_or_known_default().unwrap_or(80);
    let resolve_target = format!("{}:{}", host_str, port);
    if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&resolve_target.as_str()) {
        for addr in addrs {
            let ip = addr.ip();
            if is_private_or_reserved_ip(&ip) {
                return Err(format!(
                    "URL resolves to private/reserved IP address ({}). This is blocked for security.",
                    ip
                ));
            }
        }
    }
    // If DNS resolution fails, do a textual check as fallback
    else {
        check_host_textual(host_str)?;
    }

    Ok(())
}

/// Check if an IP address is private, loopback, link-local, or otherwise reserved
fn is_private_or_reserved_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()           // 127.0.0.0/8
            || v4.is_private()         // 10/8, 172.16/12, 192.168/16
            || v4.is_link_local()      // 169.254/16
            || v4.is_broadcast()       // 255.255.255.255
            || v4.is_unspecified()     // 0.0.0.0
            || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64  // 100.64/10 (CGNAT)
            || v4.octets()[0] == 198 && (v4.octets()[1] & 0xFE) == 18  // 198.18/15 (benchmarking)
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()           // ::1
            || v6.is_unspecified()     // ::
            // IPv6 unique local (fc00::/7)
            || (v6.segments()[0] & 0xFE00) == 0xFC00
            // IPv6 link-local (fe80::/10)
            || (v6.segments()[0] & 0xFFC0) == 0xFE80
            // IPv4-mapped IPv6 (::ffff:x.x.x.x) — check the embedded IPv4
            || {
                if let Some(v4) = v6.to_ipv4_mapped() {
                    is_private_or_reserved_ip(&std::net::IpAddr::V4(v4))
                } else {
                    false
                }
            }
        }
    }
}

/// Textual fallback for host validation when DNS resolution fails
fn check_host_textual(host: &str) -> Result<(), String> {
    let h = host.to_lowercase();
    let blocked = [
        "localhost", "127.0.0.1", "0.0.0.0", "::1", "[::1]",
    ];
    for b in blocked {
        if h == b || h.starts_with(&format!("{}.", b)) {
            return Err(format!("URL host '{}' is blocked", host));
        }
    }
    // Block known private ranges textually
    if h.starts_with("10.") || h.starts_with("192.168.") || h.starts_with("169.254.") {
        return Err(format!("URL host '{}' is in a private IP range", host));
    }
    if h.starts_with("172.") {
        if let Some(second) = h.split('.').nth(1) {
            if let Ok(octet) = second.parse::<u8>() {
                if (16..=31).contains(&octet) {
                    return Err(format!("URL host '{}' is in a private IP range", host));
                }
            }
        }
    }
    Ok(())
}

/// Sanitize yt-dlp flags using an ALLOWLIST approach.
/// Only known-safe flags are permitted. This prevents command injection and
/// data exfiltration via flags like --exec, --proxy, --cookies-from-browser,
/// --print-to-file, --load-info-json, etc.
pub(crate) fn sanitize_ytdlp_flags(flags: &[String]) -> Vec<String> {
    // Allowlist of safe yt-dlp flags (prefix-matched)
    let allowed_prefixes: &[&str] = &[
        // Format selection
        "-f", "--format", "--format-sort", "--merge-output-format",
        "--audio-format", "--audio-quality", "--video-multistreams",
        "--audio-multistreams", "--prefer-free-formats",
        // Output template
        "-o", "--output", "--restrict-filenames", "--no-overwrites",
        "--continue", "--no-continue",
        // Metadata/embedding
        "--embed-thumbnail", "--embed-metadata", "--embed-subs",
        "--embed-chapters", "--embed-info-json",
        "--write-thumbnail", "--write-subs", "--write-auto-subs",
        "--sub-lang", "--sub-format",
        // Download behavior
        "--retries", "--fragment-retries", "--buffer-size",
        "--http-chunk-size", "--concurrent-fragments",
        "--limit-rate", "--throttled-rate",
        "--sleep-interval", "--max-sleep-interval",
        // Extraction
        "--extract-audio", "-x", "--keep-video",
        "--recode-video", "--remux-video",
        // Playlist
        "--playlist-start", "--playlist-end", "--playlist-items",
        "--no-playlist", "--yes-playlist",
        // Network (safe subset)
        "--socket-timeout", "--source-address",
        // Misc safe
        "--no-warnings", "--newline", "--progress",
        "--no-mtime", "--geo-bypass",
        "--sponsorblock-mark", "--sponsorblock-remove",
        "--no-sponsorblock",
        "--age-limit",
        "--match-filter", "--no-match-filter",
    ];

    flags
        .iter()
        .filter(|f| {
            let lower = f.to_lowercase();
            // Allow flags that match our allowlist
            allowed_prefixes.iter().any(|allowed| {
                lower == *allowed || lower.starts_with(&format!("{}=", allowed))
            })
            // Also allow values (non-flag arguments that don't start with -)
            || !lower.starts_with('-')
        })
        .cloned()
        .collect()
}

/// Shell-escape a URL for safe inclusion in a shell command string.
/// Wraps in single quotes and escapes any embedded single quotes.
#[cfg(target_os = "android")]
fn shell_escape_url(url: &str) -> String {
    format!("'{}'", url.replace('\'', "'\\''"))
}

fn default_download_dir(app: &AppHandle) -> String {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        // On Android, always use shared storage so Termux can write there
        download::android_shared_download_dir()
    }

    #[cfg(target_os = "ios")]
    {
        // On iOS, use app_data_dir which is always writable
        match app.path().app_data_dir() {
            Ok(dir) => {
                let download_dir = dir.join("downloads").join("YTDL");
                if std::fs::create_dir_all(&download_dir).is_ok() {
                    return download_dir.to_string_lossy().to_string();
                }
                log::warn!("Could not create downloads subdirectory, using app_data_dir directly");
                dir.to_string_lossy().to_string()
            }
            Err(e) => {
                log::error!("Failed to get app_data_dir: {}", e);
                std::path::PathBuf::from("YTDL")
                    .to_string_lossy()
                    .to_string()
            }
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // On desktop, prefer XDG download directory, fallback to app_data_dir
        let download_dir = dirs::download_dir()
            .map(|dir| dir.join("YTDL"))
            .or_else(|| app.path().app_data_dir().ok().map(|dir| dir.join("downloads").join("YTDL")))
            .unwrap_or_else(|| {
                // Fallback to cache_dir or temp
                app.path().cache_dir()
                    .or_else(|_| app.path().temp_dir())
                    .unwrap_or_else(|_| std::path::PathBuf::from("YTDL"))
            });
        
        // Try to create the directory
        if let Err(e) = std::fs::create_dir_all(&download_dir) {
            log::warn!("Failed to create download directory '{}': {}", download_dir.display(), e);
        }
        
        download_dir.to_string_lossy().to_string()
    }
}

async fn emit_rss_sync_progress(
    app: &AppHandle,
    feed_id: &str,
    phase: &str,
    processed: usize,
    total: usize,
    message: Option<String>,
) {
    let progress = if total == 0 {
        if phase == "completed" { 100.0 } else { 0.0 }
    } else {
        ((processed as f64 / total as f64) * 100.0).min(100.0)
    };

    let _ = app.emit(
        "rss-sync-progress",
        serde_json::json!({
            "feedId": feed_id,
            "phase": phase,
            "processed": processed,
            "total": total,
            "progress": progress,
            "message": message,
        }),
    );
}

// ────────────────────────────────────────────────── Video Info ──────────────────────────────────────────────────

#[tauri::command]
pub async fn get_video_info(_app: AppHandle, url: String) -> Result<serde_json::Value, String> {
    // Validate URL for security
    validate_url(&url)?;

    // On Android, fetching video info requires running yt-dlp which can only work via Termux.
    // For quality selection, we use Termux background check to get JSON output.
    #[cfg(target_os = "android")]
    {
        let (installed, has_perm) = crate::android_bridge::termux_info();
        if !installed || !has_perm {
            return Err(
                "Video info requires Termux. Please complete Android setup first.".to_string(),
            );
        }

        let check_dir = crate::tool_install_commands::get_shared_check_dir();
        let output_file = format!("{}/video_info_{}.json", check_dir, uuid::Uuid::new_v4());
        let _ = std::fs::remove_file(&output_file);

        // Run yt-dlp -j URL in Termux background
        let command = format!("yt-dlp --no-warnings -J {}", shell_escape_url(&url));
        log::info!("[get_video_info] Sending to Termux: {} → {}", command, output_file);
        match crate::android_bridge::run_termux_check(&command, &output_file) {
            Ok(true) => {
                // Poll for result (Termux runs async)
                for i in 0..60 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if let Ok(content) = std::fs::read_to_string(&output_file) {
                        let trimmed = content.trim();
                        if !trimmed.is_empty() {
                            let _ = std::fs::remove_file(&output_file);
                            // Try to parse as JSON
                            if trimmed.starts_with('{') {
                                match serde_json::from_str::<serde_json::Value>(trimmed) {
                                    Ok(json) => {
                                        // Convert to our VideoInfo format
                                        let info = download::parse_video_info_json(&json)
                                            .map_err(|e| format!("Failed to parse video info: {}", e))?;
                                        return serde_json::to_value(&info)
                                            .map_err(|e| e.to_string());
                                    }
                                    Err(e) => {
                                        log::warn!("[get_video_info] Invalid JSON from Termux: {}", e);
                                        return Err(format!(
                                            "yt-dlp returned invalid JSON: {}",
                                            &trimmed[..trimmed.len().min(200)]
                                        ));
                                    }
                                }
                            } else {
                                // Error message from yt-dlp
                                return Err(format!("yt-dlp error: {}", trimmed));
                            }
                        }
                    }
                    // Log progress every 5 seconds
                    if i > 0 && i % 10 == 0 {
                        log::info!("[get_video_info] Still waiting for Termux response... {}s", i / 2);
                    }
                }
                let _ = std::fs::remove_file(&output_file);
                return Err("Timed out waiting for video info from Termux (30s)".to_string());
            }
            Ok(false) => {
                return Err("Failed to send command to Termux. Is it running?".to_string());
            }
            Err(e) => {
                return Err(format!("Termux bridge error: {}", e));
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        let ytdlp = download::get_ytdlp_path(&_app);
        let info = download::fetch_video_info(&ytdlp, &url)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(&info).map_err(|e| e.to_string())
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ Downloads â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[tauri::command]
#[allow(unreachable_code)]
#[allow(unused_variables)]
pub async fn start_download(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
    url: String,
    format_id: Option<String>,
) -> Result<String, String> {
    // Validate URL for security
    validate_url(&url)?;

    // ── Android: delegate to Termux ──────────────────────────────────────────
    // On Android, bundled Linux ARM64 binaries can't run due to ELF interpreter
    // mismatch (Android uses /system/bin/linker64, not /lib/ld-linux-aarch64.so.1).
    // Direct cross-app binary access is also blocked by SELinux.
    // The only working approach is Termux RUN_COMMAND Intent.
    #[cfg(target_os = "android")]
    {
        let (installed, has_perm) = crate::android_bridge::termux_info();
        if !installed {
            return Err(
                "On Android, downloads require Termux (F-Droid version). \
                 Please install Termux and complete the setup."
                    .to_string(),
            );
        }
        if !has_perm {
            return Err(
                "Termux RUN_COMMAND permission not granted. \
                 In Termux, run: echo 'allow-external-apps=true' >> ~/.termux/termux.properties \
                 Then restart Termux completely."
                    .to_string(),
            );
        }

        // Use shared storage dir (Termux can access shared storage)
        let output_dir = {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            db_lock
                .get_setting("download_path")
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| download::android_shared_download_dir())
        };

        let termux_output = if output_dir.starts_with("/data/data/")
            || output_dir.starts_with("/data/user/")
        {
            download::android_shared_download_dir()
        } else {
            output_dir
        };

        let format = format_id
            .clone()
            .unwrap_or_else(|| "bestvideo+bestaudio/best".to_string());

        let extra_args = {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            let flags_str = db_lock
                .get_setting("ytdlp_flags")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if flags_str.is_empty() {
                vec![]
            } else {
                let raw: Vec<String> =
                    flags_str.split_whitespace().map(String::from).collect();
                sanitize_ytdlp_flags(&raw)
            }
        };

        // Generate ID before launching so we can pass it to Termux for sentinel file
        let id = uuid::Uuid::new_v4().to_string();

        match crate::android_bridge::run_termux_download(
            &url,
            &termux_output,
            &format,
            &extra_args,
            &id,
        ) {
            Ok(true) => {
                log::info!("[start_download] Android: launched via Termux for {}", url);
                {
                    let db_lock = db.lock().map_err(|e| e.to_string())?;
                    let title = format!("Termux: {}", url.chars().take(60).collect::<String>());
                    let _ = db_lock.insert_download(&id, &url, &title, "");
                    let _ = db_lock.update_download_status(&id, "downloading");
                }

                // Spawn background poller to detect Termux download completion
                let app_clone = app.clone();
                let db_ref = db.inner().clone();
                let poll_id = id.clone();
                let poll_output_dir = termux_output.clone();
                tokio::spawn(async move {
                    poll_termux_download_status(
                        &app_clone,
                        &db_ref,
                        &poll_id,
                        &poll_output_dir,
                    ).await;
                });

                return Ok(id);
            }
            Ok(false) => {
                return Err(
                    "Failed to send download command to Termux. \
                     Make sure Termux is running and has external apps enabled."
                        .to_string(),
                );
            }
            Err(e) => {
                return Err(format!("Termux bridge error: {}", e));
            }
        }
    }

    // ── Desktop / non-Android path ───────────────────────────────────────────
    // On Android the cfg block above returns from all match arms.
    // `id` must be defined unconditionally so the rest of this function
    // type-checks on all targets; unreachable_code is suppressed above.
    let id = uuid::Uuid::new_v4().to_string();

    let ytdlp = download::get_ytdlp_path(&app);
    let ffmpeg = download::get_ffmpeg_path(&app);

    let info = download::fetch_video_info(&ytdlp, &url)
        .await
        .map_err(|e| e.to_string())?;

    // Check for duplicates using O(1) SQL query instead of loading all rows
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let format_to_check = format_id.as_deref().unwrap_or("");
        if let Some(status) = db_lock.download_exists_by_url(&url, format_to_check)
            .map_err(|e| e.to_string())? {
            return Err(format!("This video with the same quality is already {}", status));
        }
    }

    let download_dir = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock
            .get_setting("download_path")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| default_download_dir(&app))
    };

    std::fs::create_dir_all(&download_dir).map_err(|e| e.to_string())?;

    // Get embed settings
    let (embed_thumb, embed_meta, browser_cookies) = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let thumb = db_lock
            .get_setting("embed_thumbnail")
            .unwrap_or(None)
            .unwrap_or_else(|| "true".to_string());
        let meta = db_lock
            .get_setting("embed_metadata")
            .unwrap_or(None)
            .unwrap_or_else(|| "true".to_string());
        let cookies = db_lock
            .get_setting("browser_cookies")
            .unwrap_or(None)
            .unwrap_or_else(|| "none".to_string());
        (thumb, meta, cookies)
    };

    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock
            .insert_download(&id, &url, &info.title, &info.thumbnail)
            .map_err(|e| e.to_string())?;
        db_lock
            .update_download_status(&id, "downloading")
            .map_err(|e| e.to_string())?;
    }

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<DownloadProgress>(32);

    {
        let mut dm = dl.lock().await;
        dm.active.insert(
            id.clone(),
            download::ActiveDownload {
                id: id.clone(),
                url: url.clone(),
                status: "downloading".to_string(),
                cancel_token: cancel_tx,
            },
        );
    }

    let app_clone = app.clone();
    let id_clone = id.clone();
    let db_ref = db.inner().clone();

    let app_for_progress = app.clone();
    let id_for_progress = id.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = app_for_progress.emit("download-progress", &progress);
            // Also update DB periodically
            if let Ok(db_lock) = db_ref.lock() {
                let _ = db_lock.update_download_progress(
                    &id_for_progress,
                    progress.progress,
                    &progress.speed,
                    &progress.eta,
                );
            }
        }
    });

    let dl_arc = dl.inner().clone();
    let mut extra_args: Vec<String> = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let flags = db_lock
            .get_setting("ytdlp_flags")
            .unwrap_or(None)
            .unwrap_or_default();
        if flags.is_empty() {
            vec![]
        } else {
            let raw: Vec<String> = flags.split_whitespace().map(String::from).collect();
            sanitize_ytdlp_flags(&raw)
        }
    };

    // Add embed options
    if embed_thumb == "true" {
        extra_args.push("--embed-thumbnail".to_string());
    }
    if embed_meta == "true" {
        extra_args.push("--embed-metadata".to_string());
    }
    if browser_cookies != "none" && !browser_cookies.is_empty() {
        extra_args.push("--cookies-from-browser".to_string());
        extra_args.push(browser_cookies);
    }

    let db_for_result = db.inner().clone();

    tokio::spawn(async move {
        let result = download::run_download(
            &ytdlp,
            &ffmpeg,
            &url,
            &download_dir,
            format_id.as_deref(),
            &extra_args,
            progress_tx,
            cancel_rx,
            id_clone.clone(),
        )
        .await;

        {
            let mut dm = dl_arc.lock().await;
            dm.active.remove(&id_clone);
        }

        match result {
            Ok(file_path) => {
                // Update DB
                if let Ok(db_lock) = db_for_result.lock() {
                    let file_size = std::fs::metadata(&file_path)
                        .map(|m| m.len() as i64)
                        .unwrap_or(0);
                    let _ = db_lock.update_download_complete(&id_clone, &file_path, file_size);
                }
                let _ = app_clone.emit(
                    "download-complete",
                    serde_json::json!({ "id": id_clone, "outputPath": file_path }),
                );
            }
            Err(e) => {
                if let Ok(db_lock) = db_for_result.lock() {
                    let _ = db_lock.update_download_error(&id_clone, &e.to_string());
                }
                let _ = app_clone.emit(
                    "download-error",
                    serde_json::json!({ "id": id_clone, "error": e.to_string() }),
                );
            }
        }
    });

    Ok(id)
}

pub async fn start_download_existing(
    app: AppHandle,
    db: Arc<Mutex<Database>>,
    dl: Arc<tokio::sync::Mutex<DownloadManager>>,
    id: String,
    url: String,
    format_id: Option<String>,
) -> Result<(), String> {
    validate_url(&url)?;

    let ytdlp = download::get_ytdlp_path(&app);
    let ffmpeg = download::get_ffmpeg_path(&app);

    let download_dir = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock
            .get_setting("download_path")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| default_download_dir(&app))
    };

    std::fs::create_dir_all(&download_dir).map_err(|e| e.to_string())?;

    let (embed_thumb, embed_meta, browser_cookies) = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let thumb = db_lock
            .get_setting("embed_thumbnail")
            .unwrap_or(None)
            .unwrap_or_else(|| "true".to_string());
        let meta = db_lock
            .get_setting("embed_metadata")
            .unwrap_or(None)
            .unwrap_or_else(|| "true".to_string());
        let cookies = db_lock
            .get_setting("browser_cookies")
            .unwrap_or(None)
            .unwrap_or_else(|| "none".to_string());
        (thumb, meta, cookies)
    };

    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock
            .update_download_status(&id, "downloading")
            .map_err(|e| e.to_string())?;
    }

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<DownloadProgress>(32);

    {
        let mut dm = dl.lock().await;
        dm.active.insert(
            id.clone(),
            download::ActiveDownload {
                id: id.clone(),
                url: url.clone(),
                status: "downloading".to_string(),
                cancel_token: cancel_tx,
            },
        );
    }

    let app_clone = app.clone();
    let id_clone = id.clone();
    let db_ref = db.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = app_clone.emit("download-progress", &progress);
            if let Ok(db_lock) = db_ref.lock() {
                let _ = db_lock.update_download_progress(
                    &id_clone,
                    progress.progress,
                    &progress.speed,
                    &progress.eta,
                );
            }
        }
    });

    let dl_arc = dl.clone();
    let mut extra_args: Vec<String> = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let flags = db_lock
            .get_setting("ytdlp_flags")
            .unwrap_or(None)
            .unwrap_or_default();
        if flags.is_empty() {
            vec![]
        } else {
            let raw: Vec<String> = flags.split_whitespace().map(String::from).collect();
            sanitize_ytdlp_flags(&raw)
        }
    };

    if embed_thumb == "true" {
        extra_args.push("--embed-thumbnail".to_string());
    }
    if embed_meta == "true" {
        extra_args.push("--embed-metadata".to_string());
    }
    if browser_cookies != "none" && !browser_cookies.is_empty() {
        extra_args.push("--cookies-from-browser".to_string());
        extra_args.push(browser_cookies);
    }

    let db_for_result = db.clone();
    let app_for_result = app.clone();
    let id_for_result = id.clone();
    tokio::spawn(async move {
        let result = download::run_download(
            &ytdlp,
            &ffmpeg,
            &url,
            &download_dir,
            format_id.as_deref(),
            &extra_args,
            progress_tx,
            cancel_rx,
            id_for_result.clone(),
        )
        .await;

        {
            let mut dm = dl_arc.lock().await;
            dm.active.remove(&id_for_result);
        }

        match result {
            Ok(file_path) => {
                if let Ok(db_lock) = db_for_result.lock() {
                    let file_size = std::fs::metadata(&file_path)
                        .map(|m| m.len() as i64)
                        .unwrap_or(0);
                    let _ = db_lock.update_download_complete(&id_for_result, &file_path, file_size);
                }
                let _ = app_for_result.emit(
                    "download-complete",
                    serde_json::json!({ "id": id_for_result, "outputPath": file_path }),
                );
            }
            Err(e) => {
                if let Ok(db_lock) = db_for_result.lock() {
                    let _ = db_lock.update_download_error(&id_for_result, &e.to_string());
                }
                let _ = app_for_result.emit(
                    "download-error",
                    serde_json::json!({ "id": id_for_result, "error": e.to_string() }),
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn pause_download(
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
    id: String,
) -> Result<(), String> {
    // Cancel the running yt-dlp process (yt-dlp supports --continue, so partial files are resumable)
    {
        let dm = dl.lock().await;
        if let Some(active) = dm.active.get(&id) {
            let _ = active.cancel_token.send(true);
        }
    }
    // Update DB status to paused
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock
        .update_download_status(&id, "paused")
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn resume_download(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
    id: String,
) -> Result<(), String> {
    let (url, format_id) = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let downloads = db_lock.get_downloads().map_err(|e| e.to_string())?;
        let dl_entry = downloads
            .iter()
            .find(|d| d["id"].as_str() == Some(&id))
            .ok_or_else(|| "Download not found".to_string())?;
        let url = dl_entry["url"].as_str().map(String::from)
            .ok_or_else(|| "Download URL not found".to_string())?;
        let format_id = dl_entry["formatId"].as_str()
            .filter(|s| !s.is_empty())
            .map(String::from);
        (url, format_id)
    };
    // Use start_download_existing to reuse the same download ID instead of creating a duplicate
    let db_arc = db.inner().clone();
    let dl_arc = dl.inner().clone();
    start_download_existing(app, db_arc, dl_arc, id, url, format_id).await?;
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
    id: String,
) -> Result<(), String> {
    {
        let dm = dl.lock().await;
        if let Some(active) = dm.active.get(&id) {
            let _ = active.cancel_token.send(true);
        }
    }
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock
        .update_download_status(&id, "cancelled")
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn retry_download(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
    id: String,
) -> Result<(), String> {
    resume_download(app, db, dl, id).await
}

fn extract_path_from_ytdlp_log_line(value: &str) -> String {
    let trimmed = value.trim();

    if let Some(idx) = trimmed.find(" Merging formats into ") {
        let maybe = trimmed[idx + " Merging formats into ".len()..].trim();
        if let Some(stripped) = maybe.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return stripped.to_string();
        }
        return maybe.to_string();
    }

    if let Some(idx) = trimmed.find(" Destination: ") {
        return trimmed[idx + " Destination: ".len()..].trim().to_string();
    }

    if let Some(idx) = trimmed.find(" has already been downloaded") {
        let maybe = trimmed[..idx].trim();
        let maybe = maybe.strip_prefix("[download]").unwrap_or(maybe).trim();
        if !maybe.is_empty() {
            return maybe.to_string();
        }
    }

    if let Some(stripped) = trimmed.strip_prefix("Merging formats into ") {
        let stripped = stripped.trim();
        if let Some(inner) = stripped
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
        {
            return inner.to_string();
        }
        return stripped.to_string();
    }

    if let Some(stripped) = trimmed.strip_prefix("Destination: ") {
        return stripped.trim().to_string();
    }

    trimmed.to_string()
}

fn normalize_user_path(raw_path: &str) -> String {
    let mut normalized = extract_path_from_ytdlp_log_line(raw_path)
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string();

    if let Some(rest) = normalized.strip_prefix("file:///") {
        #[cfg(target_os = "windows")]
        {
            normalized = rest.replace('/', "\\");
        }
        #[cfg(not(target_os = "windows"))]
        {
            normalized = format!("/{}", rest);
        }
    } else if let Some(rest) = normalized.strip_prefix("file://") {
        normalized = rest.to_string();
    }

    normalized
}

/// Sanitize filename the same way yt-dlp does on Windows
/// Replaces problematic characters that Windows doesn't allow in filenames
fn sanitize_filename_for_windows(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            // Fullwidth problematic chars (common in Asian filenames)
            '｜' | '？' | '：' | '＊' | '＜' | '＞' | '＂' | '／' | '＼' => '_',
            // Standard problematic chars
            '|' | '?' | ':' | '*' | '<' | '>' | '"' | '/' | '\\' => '_',
            // Other special cases
            '\u{202E}' | '\u{202D}' => '_', // Right-to-left override
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn find_similar_file(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let parent = path.parent()?;
    let target_stem = path.file_stem()?.to_str()?.to_lowercase();
    let target_ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    log::info!("[find_similar_file] Searching for: stem='{}', ext='{}'", target_stem, target_ext);

    // Try sanitized version of the filename (yt-dlp replaces problematic chars)
    let sanitized_stem = sanitize_filename_for_windows(&target_stem);
    log::info!("[find_similar_file] Sanitized stem: '{}'", sanitized_stem);

    let mut best_match: Option<(std::path::PathBuf, usize)> = None;

    for entry in std::fs::read_dir(parent).ok()? {
        let entry = entry.ok()?;
        let candidate = entry.path();
        if !candidate.is_file() {
            continue;
        }

        let candidate_name = candidate.file_name()?.to_str()?;
        let stem = candidate.file_stem()?.to_str()?.to_lowercase();
        let ext = candidate
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        // Exact match (best case)
        if stem == target_stem && ext == target_ext {
            log::info!("[find_similar_file] Exact match found: '{}'", candidate_name);
            return Some(candidate);
        }

        // Sanitized match (yt-dlp renamed the file)
        if stem == sanitized_stem && ext == target_ext {
            log::info!("[find_similar_file] Sanitized match found: '{}'", candidate_name);
            return Some(candidate);
        }

        // Extension must match for partial matches
        if ext != target_ext {
            continue;
        }

        // Calculate similarity score (how many chars match)
        let similarity = stem
            .chars()
            .zip(target_stem.chars())
            .filter(|(a, b)| a == b)
            .count();

        // Also try against sanitized version
        let sanitized_similarity = stem
            .chars()
            .zip(sanitized_stem.chars())
            .filter(|(a, b)| a == b)
            .count();

        let max_similarity = similarity.max(sanitized_similarity);

        // Keep track of best partial match (at least 70% similar)
        let required_similarity = (target_stem.len() * 7) / 10;
        if max_similarity >= required_similarity {
            if let Some((_, current_best)) = &best_match {
                if max_similarity > *current_best {
                    log::info!(
                        "[find_similar_file] Better partial match: '{}' (similarity: {})",
                        candidate_name, max_similarity
                    );
                    best_match = Some((candidate.clone(), max_similarity));
                }
            } else {
                log::info!(
                    "[find_similar_file] Partial match: '{}' (similarity: {})",
                    candidate_name, max_similarity
                );
                best_match = Some((candidate.clone(), max_similarity));
            }
        }
    }

    if let Some((path, score)) = best_match {
        log::info!(
            "[find_similar_file] Returning best match with score {}: '{}'",
            score,
            path.display()
        );
        Some(path)
    } else {
        log::warn!("[find_similar_file] No similar file found");
        None
    }
}

fn find_file_recursive_by_name(
    root: &std::path::Path,
    target_names_lower: &[String],
    depth: usize,
) -> Option<std::path::PathBuf> {
    if depth == 0 || !root.exists() || !root.is_dir() {
        return None;
    }

    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let name_lower = name.to_lowercase();
                if target_names_lower.iter().any(|n| n == &name_lower) {
                    return Some(path);
                }
            }
        }
    }

    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive_by_name(&path, target_names_lower, depth - 1) {
                return Some(found);
            }
        }
    }

    None
}

fn normalize_stem_for_compare(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_space = false;

    for c in value.to_lowercase().chars() {
        let mapped = match c {
            '|' | '｜' | '?' | '？' | ':' | '：' | '*' | '＊' | '<' | '＜' | '>' | '＞' => ' ',
            '"' | '＂' | '/' | '／' | '\\' | '＼' => ' ',
            '-' | '_' | '.' => ' ',
            c if c.is_alphanumeric() => c,
            c if c.is_whitespace() => ' ',
            _ => ' ',
        };

        if mapped == ' ' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(mapped);
            prev_space = false;
        }
    }

    out.trim().to_string()
}

fn similarity_score(a: &str, b: &str) -> usize {
    let mut score = 0usize;
    for (x, y) in a.chars().zip(b.chars()) {
        if x == y {
            score += 1;
        }
    }
    if a.contains(b) || b.contains(a) {
        score += (a.len().min(b.len())) / 2;
    }
    score
}

fn find_file_recursive_fuzzy(
    root: &std::path::Path,
    target_ext: &str,
    target_stems_normalized: &[String],
    depth: usize,
) -> Option<std::path::PathBuf> {
    if depth == 0 || !root.exists() || !root.is_dir() {
        return None;
    }

    let mut best: Option<(std::path::PathBuf, usize)> = None;

    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !target_ext.is_empty() && ext != target_ext {
            continue;
        }

        let Some(candidate_stem_raw) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let candidate_stem = normalize_stem_for_compare(candidate_stem_raw);

        if candidate_stem.is_empty() {
            continue;
        }

        let mut local_best_score = 0usize;
        for target in target_stems_normalized {
            let score = similarity_score(&candidate_stem, target);
            if score > local_best_score {
                local_best_score = score;
            }
        }

        let min_target_len = target_stems_normalized
            .iter()
            .map(|s| s.len())
            .filter(|len| *len > 0)
            .min()
            .unwrap_or(0);
        let threshold = (min_target_len * 6) / 10;

        if local_best_score >= threshold && threshold > 0 {
            if let Some((_, best_score)) = &best {
                if local_best_score > *best_score {
                    best = Some((path.clone(), local_best_score));
                }
            } else {
                best = Some((path.clone(), local_best_score));
            }
        }
    }

    if let Some((path, _)) = best {
        return Some(path);
    }

    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive_fuzzy(
                &path,
                target_ext,
                target_stems_normalized,
                depth - 1,
            ) {
                return Some(found);
            }
        }
    }

    None
}

fn find_file_in_fallback_locations(
    target: &std::path::Path,
    configured_download_dir: Option<&str>,
) -> Option<std::path::PathBuf> {
    let file_name = target.file_name().and_then(|n| n.to_str())?;
    let target_stem_raw = target.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let target_ext = target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let mut target_names_lower = vec![file_name.to_lowercase()];
    let mut target_stems_normalized = vec![normalize_stem_for_compare(target_stem_raw)];

    let sanitized = sanitize_filename_for_windows(file_name);
    let sanitized_lower = sanitized.to_lowercase();
    if !sanitized_lower.is_empty() && !target_names_lower.iter().any(|n| n == &sanitized_lower) {
        target_names_lower.push(sanitized_lower);
    }

    let sanitized_stem = target
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_filename_for_windows)
        .unwrap_or_default();
    let sanitized_stem_normalized = normalize_stem_for_compare(&sanitized_stem);
    if !sanitized_stem_normalized.is_empty()
        && !target_stems_normalized
            .iter()
            .any(|s| s == &sanitized_stem_normalized)
    {
        target_stems_normalized.push(sanitized_stem_normalized);
    }

    let mut roots: Vec<std::path::PathBuf> = Vec::new();

    if let Some(parent) = target.parent() {
        if parent.exists() {
            roots.push(parent.to_path_buf());
        }
    }

    if let Some(cfg) = configured_download_dir {
        let cfg = normalize_user_path(cfg);
        let cfg_path = std::path::PathBuf::from(cfg);
        if cfg_path.exists() {
            roots.push(cfg_path);
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(downloads) = dirs::download_dir() {
            if downloads.exists() {
                roots.push(downloads);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let win_downloads = std::path::PathBuf::from(profile).join("Downloads");
            if win_downloads.exists() {
                roots.push(win_downloads);
            }
        }
    }

    roots.sort();
    roots.dedup();

    for root in roots {
        for name in &target_names_lower {
            let direct = root.join(name);
            if direct.exists() && direct.is_file() {
                return Some(direct);
            }
        }

        if let Some(found) = find_file_recursive_by_name(&root, &target_names_lower, 3) {
            return Some(found);
        }

        if let Some(found) = find_file_recursive_fuzzy(
            &root,
            &target_ext,
            &target_stems_normalized,
            4,
        ) {
            return Some(found);
        }
    }

    None
}

fn fallback_search_roots(configured_download_dir: Option<&str>) -> Vec<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();

    if let Some(cfg) = configured_download_dir {
        let cfg = normalize_user_path(cfg);
        let cfg_path = std::path::PathBuf::from(cfg);
        if cfg_path.exists() {
            roots.push(cfg_path);
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(downloads) = dirs::download_dir() {
            if downloads.exists() {
                roots.push(downloads);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let win_downloads = std::path::PathBuf::from(profile).join("Downloads");
            if win_downloads.exists() {
                roots.push(win_downloads);
            }
        }
    }

    roots.sort();
    roots.dedup();
    roots
}

fn find_file_by_title_in_fallback_locations(
    title: &str,
    configured_download_dir: Option<&str>,
) -> Option<std::path::PathBuf> {
    let normalized_title = normalize_stem_for_compare(title);
    if normalized_title.is_empty() {
        return None;
    }

    let target_stems = vec![normalized_title, normalize_stem_for_compare(&sanitize_filename_for_windows(title))];
    let roots = fallback_search_roots(configured_download_dir);

    for root in roots {
        if let Some(found) = find_file_recursive_fuzzy(&root, "", &target_stems, 4) {
            return Some(found);
        }
    }

    None
}

#[tauri::command]
pub async fn delete_download(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    let (file_path_to_delete, title_to_delete, configured_download_dir): (Option<String>, Option<String>, Option<String>) = if delete_file {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let downloads = db_lock.get_downloads().map_err(|e| e.to_string())?;
        let row = downloads.iter().find(|d| d["id"].as_str() == Some(&id));
        let file_path = row
            .and_then(|d| d["filePath"].as_str())
            .filter(|p| !p.trim().is_empty())
            .map(String::from);
        let title = row
            .and_then(|d| d["title"].as_str())
            .filter(|t| !t.trim().is_empty())
            .map(String::from);
        let download_dir = db_lock
            .get_setting("download_path")
            .map_err(|e| e.to_string())?
            .filter(|v| !v.trim().is_empty());
        (file_path, title, download_dir)
    } else {
        (None, None, None)
    };

    if delete_file {
        let mut resolved_file: Option<std::path::PathBuf> = None;
        let mut normalized_original_path = String::new();

        if let Some(ref path) = file_path_to_delete {
            log::info!("[delete_download] Attempting to delete file: {}", path);
            let normalized = normalize_user_path(path);
            normalized_original_path = normalized.clone();
            log::info!("[delete_download] Normalized path: {}", normalized);

            let mut file_path = std::path::PathBuf::from(&normalized);
            if !file_path.exists() {
                if let Some(similar) = find_similar_file(&file_path) {
                    file_path = similar;
                } else if let Some(found) = find_file_in_fallback_locations(
                    &file_path,
                    configured_download_dir.as_deref(),
                ) {
                    file_path = found;
                }
            }

            if file_path.exists() && file_path.is_file() {
                resolved_file = Some(file_path);
            }
        }

        if resolved_file.is_none() {
            if let Some(ref title) = title_to_delete {
                resolved_file = find_file_by_title_in_fallback_locations(
                    title,
                    configured_download_dir.as_deref(),
                );
            }
        }

        let file_to_delete = if let Some(path) = resolved_file {
            path
        } else {
            let details = if !normalized_original_path.is_empty() {
                normalized_original_path
            } else if let Some(title) = title_to_delete {
                format!("title='{}'", title)
            } else {
                "unknown path/title".to_string()
            };
            return Err(format!("File not found on disk: {}", details));
        };

        std::fs::remove_file(&file_to_delete)
            .map_err(|e| format!("Failed to delete file '{}': {}", file_to_delete.display(), e))?;
        log::info!("[delete_download] File deleted successfully: {}", file_to_delete.display());
    }

    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock.delete_download(&id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_downloads(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<serde_json::Value>, String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock.get_downloads().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_downloads(
    db: State<'_, Arc<Mutex<Database>>>,
    format: String,
) -> Result<String, String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    let downloads = db_lock.get_downloads().map_err(|e| e.to_string())?;

    match format.as_str() {
        "json" => {
            serde_json::to_string_pretty(&downloads).map_err(|e| e.to_string())
        }
        "csv" => {
            let mut csv = String::from("id,title,url,status,format,created_at,updated_at\n");
            for d in downloads {
                let id = d["id"].as_str().unwrap_or("");
                let title = d["title"].as_str().unwrap_or("");
                let url = d["url"].as_str().unwrap_or("");
                let status = d["status"].as_str().unwrap_or("");
                let format_label = d["formatLabel"].as_str().unwrap_or("");
                let created_at = d["createdAt"].as_str().unwrap_or("");
                let updated_at = d["updatedAt"].as_str().unwrap_or("");
                // CSV quoting with injection protection: prefix dangerous
                // leading chars (=, +, -, @, \t, \r) that spreadsheet apps
                // interpret as formulas.
                let quote_field = |s: &str| -> String {
                    let escaped = s.replace('"', "\"\"");
                    let safe = if escaped.starts_with('=') || escaped.starts_with('+')
                        || escaped.starts_with('-') || escaped.starts_with('@')
                        || escaped.starts_with('\t') || escaped.starts_with('\r')
                    {
                        format!("'{}", escaped)
                    } else {
                        escaped
                    };
                    format!("\"{}\"" , safe)
                };
                csv.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    quote_field(id), quote_field(title), quote_field(url),
                    quote_field(status), quote_field(format_label),
                    quote_field(created_at), quote_field(updated_at)
                ));
            }
            Ok(csv)
        }
        _ => Err("Unsupported format. Use 'json' or 'csv'.".to_string()),
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ Settings â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[tauri::command]
pub async fn get_settings(db: State<'_, Arc<Mutex<Database>>>) -> Result<serde_json::Value, String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock.get_all_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_setting(
    db: State<'_, Arc<Mutex<Database>>>,
    key: String,
    value: String,
) -> Result<(), String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock
        .save_setting(&key, &value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn select_directory(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        return Err("Directory selection is not supported on mobile platforms".to_string());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
    use tauri_plugin_dialog::DialogExt;
    let path = app
        .dialog()
        .file()
        .blocking_pick_folder();
    Ok(path.map(|p| p.to_string()))
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ RSS Feeds â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[tauri::command]
pub async fn get_feeds(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<serde_json::Value>, String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock.get_feeds().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_feed(db: State<'_, Arc<Mutex<Database>>>, url: String) -> Result<String, String> {
    let feed_url = rss::normalize_feed_url(&url)
        .await
        .map_err(|e| e.to_string())?;

    // Fast path: avoid long blocking operations when adding feed.
    // We try to fetch title quickly, but fallback to URL if network is slow.
    let mut title = url.trim().to_string();
    if let Ok(Ok((fetched_title, _))) = tokio::time::timeout(
        std::time::Duration::from_secs(6),
        rss::fetch_feed_items(&feed_url),
    )
    .await
    {
        if !fetched_title.trim().is_empty() {
            title = fetched_title;
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock
        .insert_feed(&id, &feed_url, &title, "")
        .map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub async fn remove_feed(db: State<'_, Arc<Mutex<Database>>>, id: String) -> Result<(), String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock.delete_feed(&id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn check_feed(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
) -> Result<Vec<serde_json::Value>, String> {
    emit_rss_sync_progress(
        &app,
        &id,
        "fetching",
        0,
        0,
        Some("Preparing channel sync".to_string()),
    )
    .await;

    let (feed_url, existing_channel_name, existing_avatar) = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let feeds = db_lock.get_feeds().map_err(|e| e.to_string())?;
        let feed = feeds
            .iter()
            .find(|f| f["id"].as_str() == Some(&id))
            .ok_or_else(|| "Feed not found".to_string())?;

        (
            feed["url"]
                .as_str()
                .map(String::from)
                .ok_or_else(|| "Feed URL not found".to_string())?,
            feed["channelName"].as_str().unwrap_or_default().to_string(),
            feed["channelAvatar"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        )
    };

    let normalized_url = rss::normalize_feed_url(&feed_url)
        .await
        .map_err(|e| e.to_string())?;

    if normalized_url != feed_url {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock
            .update_feed_url(&id, &normalized_url)
            .map_err(|e| e.to_string())?;
    }

    let (title, items) = rss::fetch_feed_items_extended(&app, &normalized_url)
        .await
        .map_err(|e| e.to_string())?;

    let total_items = items.len();
    emit_rss_sync_progress(
        &app,
        &id,
        "importing",
        0,
        total_items,
        Some(format!("Fetched {} items, starting batch import", total_items)),
    )
    .await;

    // Fetch channel avatar with fallback (before locking DB)
    let fetched_channel_avatar = rss::get_channel_avatar_with_fallback(&app, &normalized_url)
        .await
        .unwrap_or_default();
    let channel_avatar_to_store = if fetched_channel_avatar.trim().is_empty() {
        existing_avatar.clone()
    } else {
        fetched_channel_avatar
    };

    let channel_name_to_store = if title.trim().is_empty() {
        existing_channel_name.clone()
    } else {
        title.clone()
    };

    // Update last_checked and channel info in DB
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock
            .update_feed_last_checked(&id)
            .map_err(|e| e.to_string())?;
        
        // Update channel info while preserving existing values when network metadata is missing
        if !channel_name_to_store.is_empty() || !channel_avatar_to_store.is_empty() {
            db_lock
            .update_feed_channel_info(&id, &channel_name_to_store, &channel_avatar_to_store)
                .map_err(|e| e.to_string())?;
        }
    }

    // Save items to database in batches
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let feeds = db_lock.get_feeds().map_err(|e| e.to_string())?;
        let feed_exists = feeds.iter().any(|f| f["id"].as_str() == Some(&id));
        if !feed_exists {
            return Err(format!("Feed {} not found in database", id));
        }
    }

    let mut processed_count = 0usize;
    for chunk in items.chunks(RSS_SYNC_BATCH_SIZE) {
        {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            for item in chunk {
                let _ = db_lock.insert_feed_item(
                    &item.id,
                    &id,
                    &item.video_id,
                    &item.title,
                    &item.thumbnail,
                    &item.url,
                    &item.published_at,
                    &item.video_type,
                );
            }
        }

        processed_count += chunk.len();
        emit_rss_sync_progress(
            &app,
            &id,
            "importing",
            processed_count,
            total_items,
            Some(format!("Imported {} of {} items", processed_count, total_items)),
        )
        .await;

        tokio::task::yield_now().await;
    }

    let result: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            serde_json::json!({
                "id": item.id,
                "title": item.title,
                "url": item.url,
                "thumbnail": item.thumbnail,
                "publishedAt": item.published_at,
                "status": if item.downloaded { "downloaded" } else { "not_queued" },
                "videoType": item.video_type,
            })
        })
        .collect();

    emit_rss_sync_progress(
        &app,
        &id,
        "completed",
        total_items,
        total_items,
        Some(format!("Channel synced: {} items", total_items)),
    )
    .await;

    Ok(result)
}


#[tauri::command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
pub async fn open_external(url: String) -> Result<(), String> {
    // Only allow http/https URLs
    let trimmed = url.trim().to_lowercase();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Only http/https URLs are allowed".to_string());
    }

    // On Android, open::that() doesn't work — use Android Intent via JNI
    #[cfg(target_os = "android")]
    {
        return crate::android_bridge::open_url(url.trim())
            .and_then(|ok| {
                if ok { Ok(()) } else { Err("Android could not open URL".to_string()) }
            });
    }

    #[cfg(not(target_os = "android"))]
    {
        open::that(&url).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn open_path(
    db: State<'_, Arc<Mutex<Database>>>,
    path: String,
) -> Result<(), String> {
    // Validate path: block obviously malicious patterns
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Path cannot be empty".to_string());
    }
    
    log::info!("[open_path] Original path: {}", trimmed);
    let normalized = normalize_user_path(trimmed);
    log::info!("[open_path] Normalized path: {}", normalized);

    // On Android, open::that() doesn't work — use Android intents via JNI
    #[cfg(target_os = "android")]
    {
        return crate::android_bridge::open_file_path(&normalized)
            .and_then(|ok| {
                if ok { 
                    log::info!("[open_path] Successfully opened on Android: {}", normalized);
                    Ok(()) 
                } else { 
                    log::error!("[open_path] Failed to open on Android: {}", normalized);
                    Err("Could not open file on Android".to_string()) 
                }
            });
    }

    #[cfg(not(target_os = "android"))]
    {
        let configured_download_dir = db
            .lock()
            .map_err(|e| e.to_string())?
            .get_setting("download_path")
            .map_err(|e| e.to_string())?
            .filter(|v| !v.trim().is_empty());
        let target = std::path::PathBuf::from(&normalized);
        log::info!("[open_path] Checking if file exists: {}", target.display());

        if target.exists() {
            log::info!("[open_path] File exists, opening: {}", target.display());
            return open::that(&target)
                .map_err(|e| format!("Failed to open '{}': {}", target.display(), e));
        }

        if target.is_dir() {
            return open::that(&target)
                .map_err(|e| format!("Failed to open directory '{}': {}", target.display(), e));
        }

        log::warn!("[open_path] File not found, searching for similar: {}", target.display());
        if let Some(similar) = find_similar_file(&target) {
            log::info!("[open_path] Found similar file, opening: {}", similar.display());
            return open::that(&similar)
                .map_err(|e| format!("Failed to open '{}': {}", similar.display(), e));
        }

        if let Some(found) = find_file_in_fallback_locations(&target, configured_download_dir.as_deref()) {
            log::info!("[open_path] Found file in fallback location, opening: {}", found.display());
            return open::that(&found)
                .map_err(|e| format!("Failed to open '{}': {}", found.display(), e));
        }

        log::error!("[open_path] No file or directory found for: {}", normalized);
        Err(format!("Failed to open '{}': path not found", normalized))
    }
}

// ────────────────────────────────── Stream Proxy (Custom Player) ──────────────────────────────────

/// Extract direct stream URLs from a video URL using yt-dlp.
/// This allows playing videos in a custom player even in countries where YouTube is blocked,
/// because yt-dlp can use proxies/cookies and returns direct CDN URLs.
#[tauri::command]
pub async fn get_stream_url(
    _app: AppHandle,
    _db: State<'_, Arc<Mutex<Database>>>,
    url: String,
) -> Result<serde_json::Value, String> {
    validate_url(&url)?;

    // On Android, use Termux to run yt-dlp -j for stream extraction.
    #[cfg(target_os = "android")]
    {
        let (installed, has_perm) = crate::android_bridge::termux_info();
        if !installed || !has_perm {
            return Err("Stream playback requires Termux. Please complete Android setup first.".to_string());
        }

        let check_dir = crate::tool_install_commands::get_shared_check_dir();
        let output_file = format!("{}/stream_{}.json", check_dir, uuid::Uuid::new_v4());
        let _ = std::fs::remove_file(&output_file);

        let command = format!("yt-dlp --no-warnings -j --no-playlist {}", shell_escape_url(&url));
        log::info!("[get_stream_url] Sending to Termux: {}", command);

        match crate::android_bridge::run_termux_check(&command, &output_file) {
            Ok(true) => {
                // Poll for result (up to 30 seconds)
                for i in 0..60 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if let Ok(content) = std::fs::read_to_string(&output_file) {
                        let trimmed = content.trim();
                        if !trimmed.is_empty() {
                            let _ = std::fs::remove_file(&output_file);
                            if trimmed.starts_with('{') {
                                match serde_json::from_str::<serde_json::Value>(trimmed) {
                                    Ok(json) => {
                                        return parse_stream_json(&json);
                                    }
                                    Err(e) => {
                                        return Err(format!("yt-dlp returned invalid JSON: {}", e));
                                    }
                                }
                            } else {
                                return Err(format!("yt-dlp error: {}", &trimmed[..trimmed.len().min(300)]));
                            }
                        }
                    }
                    if i > 0 && i % 10 == 0 {
                        log::info!("[get_stream_url] Still waiting... {}s", i / 2);
                    }
                }
                let _ = std::fs::remove_file(&output_file);
                return Err("Timed out waiting for stream info from Termux (30s)".to_string());
            }
            Ok(false) => return Err("Failed to send command to Termux. Is it running?".to_string()),
            Err(e) => return Err(format!("Termux bridge error: {}", e)),
        }
    }

    #[cfg(not(target_os = "android"))]
    {

    let ytdlp = download::get_ytdlp_path(&_app);

    // Get browser cookies setting for bypassing restrictions
    let browser_cookies = {
        let db_lock = _db.lock().map_err(|e| e.to_string())?;
        db_lock
            .get_setting("browser_cookies")
            .unwrap_or(None)
            .unwrap_or_else(|| "none".to_string())
    };

    let mut args = vec![
        "-j".to_string(),
        "--no-download".to_string(),
        "--no-warnings".to_string(),
        "--no-playlist".to_string(),
        url.clone(),
    ];

    if browser_cookies != "none" && !browser_cookies.is_empty() {
        args.insert(0, "--cookies-from-browser".to_string());
        args.insert(1, browser_cookies);
    }

    let output = download::create_hidden_command(&ytdlp)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp failed: {}", stderr.trim()));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {}", e))?;

    parse_stream_json(&json)

    } // #[cfg(not(target_os = "android"))]
}

/// Parse yt-dlp JSON output into stream info for the video player.
/// Shared between Android (Termux) and Desktop (direct yt-dlp) code paths.
fn parse_stream_json(json: &serde_json::Value) -> Result<serde_json::Value, String> {
    // Extract best video+audio or combined format URL
    let mut video_url = String::new();
    let mut audio_url = String::new();
    let mut combined_url = String::new();
    let title = json["title"].as_str().unwrap_or("").to_string();
    let thumbnail = json["thumbnail"].as_str().unwrap_or("").to_string();
    let duration = json["duration"].as_f64().unwrap_or(0.0);
    let uploader = json["uploader"].as_str().unwrap_or("").to_string();

    // Check if there's a direct URL (for combined formats)
    if let Some(url_val) = json["url"].as_str() {
        combined_url = url_val.to_string();
    }

    // Try to get separate video and audio streams for better quality
    if let Some(formats) = json["formats"].as_array() {
        // Find best video-only stream (prefer mp4/webm)
        let mut best_video: Option<&serde_json::Value> = None;
        let mut best_video_height: i64 = 0;
        
        // Find best audio-only stream
        let mut best_audio: Option<&serde_json::Value> = None;
        let mut best_audio_tbr: f64 = 0.0;

        for f in formats {
            let vcodec = f["vcodec"].as_str().unwrap_or("none");
            let acodec = f["acodec"].as_str().unwrap_or("none");
            let height = f["height"].as_i64().unwrap_or(0);
            let tbr = f["tbr"].as_f64().unwrap_or(0.0);
            let url_str = f["url"].as_str().unwrap_or("");
            
            if url_str.is_empty() {
                continue;
            }

            // Video-only stream
            if vcodec != "none" && acodec == "none" && height > best_video_height {
                best_video = Some(f);
                best_video_height = height;
            }

            // Audio-only stream
            if acodec != "none" && vcodec == "none" && tbr > best_audio_tbr {
                best_audio = Some(f);
                best_audio_tbr = tbr;
            }

            // Combined stream (video + audio)
            if vcodec != "none" && acodec != "none" && height > 0 {
                if combined_url.is_empty() || height >= 720 {
                    combined_url = url_str.to_string();
                }
            }
        }

        if let Some(v) = best_video {
            video_url = v["url"].as_str().unwrap_or("").to_string();
        }
        if let Some(a) = best_audio {
            audio_url = a["url"].as_str().unwrap_or("").to_string();
        }
    }

    // Build list of available qualities
    let mut qualities: Vec<serde_json::Value> = Vec::new();
    if let Some(formats) = json["formats"].as_array() {
        let mut seen_heights = std::collections::HashSet::new();
        for f in formats.iter().rev() {
            let vcodec = f["vcodec"].as_str().unwrap_or("none");
            let height = f["height"].as_i64().unwrap_or(0);
            let url_str = f["url"].as_str().unwrap_or("");
            
            if vcodec == "none" || height == 0 || url_str.is_empty() {
                continue;
            }
            if seen_heights.contains(&height) {
                continue;
            }
            seen_heights.insert(height);

            qualities.push(serde_json::json!({
                "height": height,
                "url": url_str,
                "formatId": f["format_id"].as_str().unwrap_or(""),
                "fps": f["fps"].as_f64().unwrap_or(0.0),
                "ext": f["ext"].as_str().unwrap_or(""),
            }));
        }
        qualities.sort_by(|a, b| {
            let ah = a["height"].as_i64().unwrap_or(0);
            let bh = b["height"].as_i64().unwrap_or(0);
            bh.cmp(&ah)
        });
    }

    Ok(serde_json::json!({
        "videoUrl": if !video_url.is_empty() { &video_url } else { &combined_url },
        "audioUrl": audio_url,
        "combinedUrl": combined_url,
        "title": title,
        "thumbnail": thumbnail,
        "duration": duration,
        "uploader": uploader,
        "qualities": qualities,
    }))
}

// ────────────────────────────────── RSS Scheduler ──────────────────────────────────

#[tauri::command]
pub async fn set_rss_check_interval(
    scheduler: State<'_, std::sync::Arc<tokio::sync::Mutex<crate::rss_scheduler::RssScheduler>>>,
    minutes: u64,
) -> Result<(), String> {
    let scheduler = scheduler.lock().await;
    scheduler.set_interval(minutes).await;
    Ok(())
}

#[tauri::command]
pub async fn get_rss_check_interval(
    scheduler: State<'_, std::sync::Arc<tokio::sync::Mutex<crate::rss_scheduler::RssScheduler>>>,
) -> Result<u64, String> {
    let scheduler = scheduler.lock().await;
    Ok(scheduler.get_interval().await)
}

#[tauri::command]
pub async fn check_all_rss_feeds(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<u32, String> {
    let feeds = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock.get_feeds().map_err(|e| e.to_string())?
    };

    let mut updated_count = 0u32;

    for feed in feeds {
        let feed_id = feed["id"].as_str().unwrap_or_default().to_string();
        let feed_url = feed["url"].as_str().unwrap_or_default().to_string();

        if feed_url.is_empty() {
            continue;
        }

        let normalized_url = match rss::normalize_feed_url(&feed_url).await {
            Ok(url) => url,
            Err(_) => continue,
        };

        let (title, items) = match rss::fetch_feed_items_extended(&app, &normalized_url).await {
            Ok(result) => result,
            Err(_) => continue,
        };

        {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            let _ = db_lock.update_feed_last_checked(&feed_id);
            if !title.is_empty() {
                let _ = db_lock.update_feed_channel_info(&feed_id, &title, "");
            }
            for item in &items {
                let _ = db_lock.insert_feed_item(
                    &item.id,
                    &feed_id,
                    &item.video_id,
                    &item.title,
                    &item.thumbnail,
                    &item.url,
                    &item.published_at,
                    &item.video_type,
                );
            }
        }

        updated_count += 1;
    }

    let _ = app.emit("rss-updated", serde_json::json!({ "count": updated_count }));
    Ok(updated_count)
}

#[tauri::command]
pub async fn mark_feed_item_watched(
    db: State<'_, Arc<Mutex<Database>>>,
    item_id: String,
    watched: bool,
) -> Result<(), String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock
        .update_feed_item_downloaded(&item_id, watched)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn update_feed_settings(
    db: State<'_, Arc<Mutex<Database>>>,
    feed_id: String,
    keywords: String,
    auto_download: bool,
) -> Result<(), String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock
        .update_feed_settings(&feed_id, &keywords, auto_download)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn set_download_priority(
    db: State<'_, Arc<Mutex<Database>>>,
    download_id: String,
    priority: i32,
) -> Result<(), String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock
        .update_download_priority(&download_id, priority)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn pause_all_downloads(
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
) -> Result<u32, String> {
    let downloads = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock.get_downloads().map_err(|e| e.to_string())?
    };

    let active_ids: Vec<String> = downloads
        .iter()
        .filter(|d| {
            let status = d["status"].as_str().unwrap_or("");
            status == "downloading" || status == "queued" || status == "merging"
        })
        .filter_map(|d| d["id"].as_str().map(String::from))
        .collect();

    let mut paused_count = 0u32;
    for id in active_ids {
        {
            let dm = dl.lock().await;
            if let Some(active) = dm.active.get(&id) {
                let _ = active.cancel_token.send(true);
            }
        }

        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let _ = db_lock.update_download_status(&id, "paused");
        paused_count += 1;
    }

    Ok(paused_count)
}

#[tauri::command]
pub async fn resume_all_downloads(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
) -> Result<u32, String> {
    let downloads = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock.get_downloads().map_err(|e| e.to_string())?
    };

    let paused_ids: Vec<String> = downloads
        .iter()
        .filter(|d| d["status"].as_str() == Some("paused"))
        .filter_map(|d| d["id"].as_str().map(String::from))
        .collect();

    let mut resumed_count = 0u32;
    let db_arc = db.inner().clone();
    let dl_arc = dl.inner().clone();
    for id in paused_ids {
        // Get URL and format_id for this download
        let (url, format_id) = {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            let downloads = db_lock.get_downloads().map_err(|e| e.to_string())?;
            if let Some(dl_entry) = downloads.iter().find(|d| d["id"].as_str() == Some(&id)) {
                let url = dl_entry["url"].as_str().map(String::from).unwrap_or_default();
                let format_id = dl_entry["formatId"].as_str()
                    .filter(|s| !s.is_empty())
                    .map(String::from);
                (url, format_id)
            } else {
                continue;
            }
        };
        if url.is_empty() {
            continue;
        }
        // Use start_download_existing to properly restart the download process
        if start_download_existing(app.clone(), db_arc.clone(), dl_arc.clone(), id, url, format_id).await.is_ok() {
            resumed_count += 1;
        }
    }

    let _ = app.emit("downloads-resumed", serde_json::json!({ "count": resumed_count }));
    Ok(resumed_count)
}

#[tauri::command]
pub async fn cancel_all_downloads(
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
) -> Result<u32, String> {
    let downloads = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock.get_downloads().map_err(|e| e.to_string())?
    };

    let active_ids: Vec<String> = downloads
        .iter()
        .filter(|d| {
            let status = d["status"].as_str().unwrap_or("");
            status == "downloading" || status == "paused" || status == "pending"
        })
        .filter_map(|d| d["id"].as_str().map(String::from))
        .collect();

    let mut cancelled_count = 0u32;
    for id in active_ids {
        // Cancel the active download process first
        {
            let mut dl_lock = dl.lock().await;
            dl_lock.cancel(&id);
        }
        // Then update DB status (separate lock to avoid deadlock)
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let _ = db_lock.update_download_status(&id, "cancelled");
        drop(db_lock);
        cancelled_count += 1;
    }

    Ok(cancelled_count)
}

// ────────────────────────────────── Termux download metadata extraction ──────────────────────────────────

/// Scan the output directory for .info.json files written by yt-dlp --write-info-json.
/// Returns (title, thumbnail_url) extracted from the most recently modified .info.json.
#[cfg(target_os = "android")]
fn extract_info_json_metadata(output_dir: &str) -> (String, String) {
    use std::path::Path;

    let dir = Path::new(output_dir);
    if !dir.exists() {
        return (String::new(), String::new());
    }

    // Find the most recently modified .info.json file
    let mut best_path: Option<std::path::PathBuf> = None;
    let mut best_mtime = std::time::SystemTime::UNIX_EPOCH;

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.ends_with(".info.json") {
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(mtime) = meta.modified() {
                        if mtime > best_mtime {
                            best_mtime = mtime;
                            best_path = Some(path);
                        }
                    }
                }
            }
        }
    }

    let Some(json_path) = best_path else {
        log::debug!("[extract_info_json_metadata] No .info.json found in {}", output_dir);
        return (String::new(), String::new());
    };

    match std::fs::read_to_string(&json_path) {
        Ok(content) => {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json) => {
                    let title = json["title"].as_str().unwrap_or("").to_string();
                    let thumbnail = json["thumbnail"].as_str().unwrap_or("").to_string();
                    log::info!(
                        "[extract_info_json_metadata] Found metadata: title='{}' thumbnail='{}'",
                        title,
                        if thumbnail.is_empty() { "(empty)" } else { &thumbnail }
                    );
                    // Clean up the .info.json file after reading
                    let _ = std::fs::remove_file(&json_path);
                    (title, thumbnail)
                }
                Err(e) => {
                    log::warn!("[extract_info_json_metadata] Failed to parse JSON: {}", e);
                    (String::new(), String::new())
                }
            }
        }
        Err(e) => {
            log::warn!("[extract_info_json_metadata] Failed to read file: {}", e);
            (String::new(), String::new())
        }
    }
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
fn extract_info_json_metadata(_output_dir: &str) -> (String, String) {
    (String::new(), String::new())
}

// ────────────────────────────────── Termux download completion poller ──────────────────────────────────

/// Poll the sentinel file written by Termux's shell command after yt-dlp finishes.
/// Updates the database and emits events so the frontend UI reflects the real status.
///
/// The sentinel file lives at `<output_dir>/.status/<download_id>` and contains:
/// - Line 1: "OK" (success) or "FAIL:<exit_code>" (error)
/// - Line 2 (optional, success only): absolute path to the most recently modified file
#[cfg(target_os = "android")]
async fn poll_termux_download_status(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Database>>,
    download_id: &str,
    output_dir: &str,
) {
    use std::path::Path;

    let status_file = format!("{}/.status/{}", output_dir, download_id);
    let poll_interval = std::time::Duration::from_secs(3);
    // Maximum wait: 4 hours (enough for even very large downloads)
    let max_polls = 4 * 60 * 60 / 3;

    log::info!(
        "[poll_termux] Starting poller for download {} — watching {}",
        download_id,
        status_file
    );

    for i in 0..max_polls {
        tokio::time::sleep(poll_interval).await;

        // Check if download was cancelled by the user in the app
        {
            if let Ok(db_lock) = db.lock() {
                if let Ok(downloads) = db_lock.get_downloads() {
                    if let Some(dl) = downloads.iter().find(|d| d["id"].as_str() == Some(download_id)) {
                        let status = dl["status"].as_str().unwrap_or("");
                        if status == "cancelled" || status == "completed" || status == "error" {
                            log::info!("[poll_termux] Download {} already in terminal state '{}', stopping poller", download_id, status);
                            // Clean up sentinel file if it exists
                            let _ = std::fs::remove_file(&status_file);
                            return;
                        }
                    }
                }
            }
        }

        if !Path::new(&status_file).exists() {
            // Log every ~30 seconds
            if i > 0 && i % 10 == 0 {
                log::debug!("[poll_termux] Still waiting for {} ({}s)", download_id, i * 3);
            }
            continue;
        }

        // Sentinel file found — read contents
        let contents = match std::fs::read_to_string(&status_file) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[poll_termux] Failed to read sentinel file: {}", e);
                // Try again next poll
                continue;
            }
        };

        let lines: Vec<&str> = contents.lines().collect();
        let status_line = lines.first().unwrap_or(&"").trim();

        if status_line == "OK" || status_line.starts_with("OK") {
            // Success! Try to get the output file path
            let file_path = lines.get(1).map(|s| s.trim().to_string()).unwrap_or_default();

            let file_size = if !file_path.is_empty() {
                std::fs::metadata(&file_path)
                    .map(|m| m.len() as i64)
                    .unwrap_or(0)
            } else {
                0
            };

            log::info!(
                "[poll_termux] Download {} completed! file={} size={}",
                download_id,
                file_path,
                file_size
            );

            // Try to extract metadata from .info.json files in the output dir
            let (meta_title, meta_thumbnail) = extract_info_json_metadata(output_dir);

            // Update DB
            if let Ok(db_lock) = db.lock() {
                let _ = db_lock.update_download_complete(download_id, &file_path, file_size);
                // Update title/thumbnail if we found metadata
                if !meta_title.is_empty() || !meta_thumbnail.is_empty() {
                    let final_title = if !meta_title.is_empty() { &meta_title } else { &file_path };
                    let _ = db_lock.update_download_metadata(download_id, final_title, &meta_thumbnail);
                } else if !file_path.is_empty() {
                    // Fallback: use filename as title
                    let filename = Path::new(&file_path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !filename.is_empty() {
                        let _ = db_lock.update_download_metadata(download_id, &filename, "");
                    }
                }
            }

            // Emit events for frontend
            let _ = app.emit(
                "download-complete",
                serde_json::json!({
                    "id": download_id,
                    "outputPath": file_path,
                }),
            );
        } else if status_line.starts_with("FAIL") {
            let error_msg = format!("yt-dlp exited with error ({})", status_line);
            log::warn!("[poll_termux] Download {} failed: {}", download_id, error_msg);

            if let Ok(db_lock) = db.lock() {
                let _ = db_lock.update_download_error(download_id, &error_msg);
            }

            let _ = app.emit(
                "download-error",
                serde_json::json!({
                    "id": download_id,
                    "error": error_msg,
                }),
            );
        } else {
            // Unknown status — treat as completed (sentinel exists => yt-dlp finished)
            log::warn!("[poll_termux] Unknown sentinel content for {}: {:?}", download_id, status_line);

            if let Ok(db_lock) = db.lock() {
                let _ = db_lock.update_download_status(download_id, "completed");
                let _ = db_lock.update_download_progress(download_id, 100.0, "", "");
            }

            let _ = app.emit(
                "download-complete",
                serde_json::json!({
                    "id": download_id,
                    "outputPath": "",
                }),
            );
        }

        // Clean up sentinel file
        let _ = std::fs::remove_file(&status_file);
        log::info!("[poll_termux] Poller finished for download {}", download_id);
        return;
    }

    // Timeout — mark as completed (the download probably finished but we can't confirm)
    log::warn!("[poll_termux] Poller timed out for download {} after 4 hours", download_id);
    if let Ok(db_lock) = db.lock() {
        // Don't mark as error — the download likely succeeded in Termux
        let _ = db_lock.update_download_status(download_id, "completed");
        let _ = db_lock.update_download_progress(download_id, 100.0, "", "");
    }
    let _ = app.emit(
        "download-complete",
        serde_json::json!({
            "id": download_id,
            "outputPath": "",
        }),
    );
}

/// Stub for non-Android — never called but keeps the code compiling.
#[cfg(not(target_os = "android"))]
async fn poll_termux_download_status(
    _app: &tauri::AppHandle,
    _db: &Arc<Mutex<Database>>,
    _download_id: &str,
    _output_dir: &str,
) {}

/// Public re-export for use by `android_commands.rs`.
pub async fn poll_termux_download_status_pub(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Database>>,
    download_id: &str,
    output_dir: &str,
) {
    poll_termux_download_status(app, db, download_id, output_dir).await;
}

