use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, process::Stdio};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Database;
use crate::download::{self, DownloadManager, DownloadProgress};
use crate::rss;

const RSS_SYNC_BATCH_SIZE: usize = 200;

/// Validates a URL for security (SSRF protection)
pub fn validate_url(url: &str) -> Result<(), String> {
    // Check if URL is not empty
    if url.trim().is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    // Check for minimum length
    if url.len() < 10 {
        return Err("URL is too short".to_string());
    }

    // Check for valid URL schemes
    let trimmed = url.trim().to_lowercase();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }

    // Block internal/private IPs and localhost
    let blocked_patterns = [
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "192.168.",
        "169.254.",
        "::1",
        "[::1]",
        "file://",
    ];

    for pattern in blocked_patterns {
        if trimmed.contains(pattern) {
            return Err(format!("URL contains blocked pattern: {}", pattern));
        }
    }

    // Block all 10.x.x.x private range (not just 10.0.)
    if let Some(host_start) = trimmed.find("://") {
        let after_scheme = &trimmed[host_start + 3..];
        let host = after_scheme.split('/').next().unwrap_or("");
        let host = host.split(':').next().unwrap_or(""); // strip port
        // Block 10.0.0.0/8
        if host.starts_with("10.") {
            return Err("URL contains private IP range (10.x.x.x)".to_string());
        }
        // Block 172.16.0.0/12 (172.16.x.x - 172.31.x.x)
        if host.starts_with("172.") {
            if let Some(second_octet) = host.split('.').nth(1) {
                if let Ok(octet) = second_octet.parse::<u8>() {
                    if (16..=31).contains(&octet) {
                        return Err("URL contains private IP range (172.16-31.x.x)".to_string());
                    }
                }
            }
        }
    }

    Ok(())
}

/// Sanitize yt-dlp flags to block dangerous options
fn sanitize_ytdlp_flags(flags: &[String]) -> Vec<String> {
    let dangerous = [
        "--exec", "--exec-before-download", "--batch-file",
        "--config-locations", "--config-location", "--output-na-placeholder",
        "-a", "--download-archive",
    ];
    flags
        .iter()
        .filter(|f| {
            let lower = f.to_lowercase();
            !dangerous.iter().any(|d| lower.starts_with(d))
        })
        .cloned()
        .collect()
}

fn default_download_dir(_app: &AppHandle) -> String {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        return _app
            .path()
            .app_data_dir()
            .unwrap_or_default()
            .join("downloads")
            .join("YTDL")
            .to_string_lossy()
            .to_string();
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        dirs::download_dir()
            .unwrap_or_default()
            .join("YTDL")
            .to_string_lossy()
            .to_string()
    }
}

fn ensure_tool_bin_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let bin_dir = download::get_binary_dir(app);
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    Ok(bin_dir)
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

async fn wait_for_cancel(mut rx: tokio::sync::watch::Receiver<bool>) {
    if *rx.borrow() {
        return;
    }
    while rx.changed().await.is_ok() {
        if *rx.borrow() {
            break;
        }
    }
}

// ────────────────────────────────────────────────── Video Info ──────────────────────────────────────────────────

#[tauri::command]
pub async fn get_video_info(app: AppHandle, url: String) -> Result<serde_json::Value, String> {
    // Validate URL for security
    validate_url(&url)?;

    let ytdlp = download::get_ytdlp_path(&app);
    let info = download::fetch_video_info(&ytdlp, &url)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&info).map_err(|e| e.to_string())
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ Downloads â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
    url: String,
    format_id: Option<String>,
) -> Result<String, String> {
    // Validate URL for security
    validate_url(&url)?;

    let id = uuid::Uuid::new_v4().to_string();

    let ytdlp = download::get_ytdlp_path(&app);
    let ffmpeg = download::get_ffmpeg_path(&app);

    let info = download::fetch_video_info(&ytdlp, &url)
        .await
        .map_err(|e| e.to_string())?;

    // Check for duplicates: same URL and format_id
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let downloads = db_lock.get_downloads().map_err(|e| e.to_string())?;
        let format_to_check = format_id.as_deref().unwrap_or("");
        for dl in downloads.iter() {
            if dl["url"].as_str() == Some(&url) 
                && dl["formatId"].as_str().unwrap_or("") == format_to_check
                && (dl["status"].as_str() == Some("completed") 
                    || dl["status"].as_str() == Some("downloading")
                    || dl["status"].as_str() == Some("queued")) {
                return Err(format!("This video with the same quality is already {}", 
                    dl["status"].as_str().unwrap_or("in queue")));
            }
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
    let dm = dl.lock().await;
    if let Some(active) = dm.active.get(&id) {
        let _ = active.cancel_token.send(true);
    }
    drop(dm);
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
    start_download(app, db, dl, url, format_id).await?;
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

#[tauri::command]
pub async fn delete_download(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    // First, get the file path while holding the lock, then release it
    let file_path_to_delete: Option<String> = if delete_file {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let downloads = db_lock.get_downloads().map_err(|e| e.to_string())?;
        downloads.iter()
            .find(|d| d["id"].as_str() == Some(&id))
            .and_then(|d| d["filePath"].as_str())
            .filter(|p| !p.is_empty())
            .map(String::from)
        // db_lock is dropped here
    } else {
        None
    };
    
    // Delete the file outside of the database lock
    if let Some(ref path) = file_path_to_delete {
        let file_path = std::path::Path::new(path);
        if file_path.exists() {
            std::fs::remove_file(file_path)
                .map_err(|e| format!("Failed to delete file '{}': {}", path, e))?;
        }
    }
    
    // Now delete from database
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
                // Proper CSV quoting: wrap in double-quotes, escape embedded quotes
                let quote_field = |s: &str| -> String {
                    let escaped = s.replace('"', "\"\"");
                    format!("\"{}\"" , escaped)
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

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ Transcription â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[tauri::command]
pub async fn start_transcription(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    transcription_jobs: State<'_, Arc<tokio::sync::Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>>>,
    source: String,
    model_size: Option<String>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let model_override = model_size.unwrap_or_default();
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    {
        let mut jobs = transcription_jobs.lock().await;
        jobs.insert(id.clone(), cancel_tx);
    }

    let (provider, api_key, api_model, whisper_cpp, whisper_model) = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let provider = db_lock
            .get_setting("transcribe_provider")
            .unwrap_or(None)
            .unwrap_or_else(|| "api".to_string());
        let api_key = db_lock
            .get_setting("openai_api_key")
            .unwrap_or(None)
            .unwrap_or_default();
        let api_model = db_lock
            .get_setting("openai_model")
            .unwrap_or(None)
            .unwrap_or_else(|| "whisper-1".to_string());
        let whisper_cpp = db_lock
            .get_setting("whisper_cpp_path")
            .unwrap_or(None)
            .unwrap_or_default();
        let whisper_model = db_lock
            .get_setting("whisper_model_path")
            .unwrap_or(None)
            .unwrap_or_default();
        (provider, api_key, api_model, whisper_cpp, whisper_model)
    };

    // Insert transcript record into DB
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock
            .insert_transcript(&id, &source, &source)
            .map_err(|e| e.to_string())?;
        db_lock
            .update_transcript_status(&id, "processing", 0.0)
            .map_err(|e| e.to_string())?;
    }

    let app_clone = app.clone();
    let id_clone = id.clone();
    let source_clone = source.clone();
    let db_clone = db.inner().clone();
    let provider_clone = provider.clone();
    let api_key_clone = api_key.clone();
    let api_model_clone = api_model.clone();
    let whisper_cpp_clone = whisper_cpp.clone();
    let whisper_model_clone = whisper_model.clone();
    let model_override_clone = model_override.clone();
    let transcription_jobs_clone = transcription_jobs.inner().clone();
    let cancel_rx_clone = cancel_rx.clone();

    tokio::spawn(async move {
        let run = async {
        let _ = app_clone.emit(
            "transcription-progress",
            serde_json::json!({
                "id": id_clone,
                "progress": 0.0,
                "status": "processing"
            }),
        );

        let mut temp_files: Vec<PathBuf> = Vec::new();
        let audio_path = if source_clone.starts_with("http://")
            || source_clone.starts_with("https://")
        {
            let temp_dir = match app_clone.path().temp_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    if let Ok(db_lock) = db_clone.lock() {
                        let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                    }
                    let _ = app_clone.emit(
                        "transcription-progress",
                        serde_json::json!({
                            "id": id_clone,
                            "progress": 0.0,
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                    return;
                }
            };

            let base = temp_dir.join(format!("transcribe-{}", id_clone));
            let output_template = format!("{}.%(ext)s", base.to_string_lossy());
            let output_audio = base.with_extension("mp3");

            let ytdlp = download::get_ytdlp_path(&app_clone);
            let output = download::create_hidden_command(&ytdlp)
                .args([
                    "-x",
                    "--audio-format",
                    "mp3",
                    "--audio-quality",
                    "0",
                    "--no-warnings",
                    "--no-playlist",
                    "-o",
                    &output_template,
                    &source_clone,
                ])
                .output()
                .await;

            match output {
                Ok(result) => {
                    if !result.status.success() {
                        let stderr = String::from_utf8_lossy(&result.stderr);
                        if let Ok(db_lock) = db_clone.lock() {
                            let _ = db_lock.update_transcript_error(&id_clone, stderr.trim());
                        }
                        let _ = app_clone.emit(
                            "transcription-progress",
                            serde_json::json!({
                                "id": id_clone,
                                "progress": 0.0,
                                "status": "error",
                                "error": stderr.trim()
                            }),
                        );
                        return;
                    }
                }
                Err(e) => {
                    if let Ok(db_lock) = db_clone.lock() {
                        let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                    }
                    let _ = app_clone.emit(
                        "transcription-progress",
                        serde_json::json!({
                            "id": id_clone,
                            "progress": 0.0,
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                    return;
                }
            }

            if !output_audio.exists() {
                let err = "Audio download failed: output file not found";
                if let Ok(db_lock) = db_clone.lock() {
                    let _ = db_lock.update_transcript_error(&id_clone, err);
                }
                let _ = app_clone.emit(
                    "transcription-progress",
                    serde_json::json!({
                        "id": id_clone,
                        "progress": 0.0,
                        "status": "error",
                        "error": err
                    }),
                );
                return;
            }

            temp_files.push(output_audio.clone());
            output_audio
        } else {
            PathBuf::from(source_clone)
        };

        let (text, language) = if provider_clone == "local" {
            if whisper_cpp_clone.is_empty() || whisper_model_clone.is_empty() {
                let err = "Local transcription requires whisper_cpp_path and whisper_model_path";
                if let Ok(db_lock) = db_clone.lock() {
                    let _ = db_lock.update_transcript_error(&id_clone, err);
                }
                let _ = app_clone.emit(
                    "transcription-progress",
                    serde_json::json!({
                        "id": id_clone,
                        "progress": 0.0,
                        "status": "error",
                        "error": err
                    }),
                );
                return;
            }

            let mut local_audio_path = audio_path.clone();
            let local_ext = audio_path
                .extension()
                .and_then(|v| v.to_str())
                .map(|v| v.to_lowercase())
                .unwrap_or_default();
            let is_audio_like = ["mp3", "wav", "m4a", "flac", "ogg", "opus", "aac", "wma"]
                .contains(&local_ext.as_str());

            if !is_audio_like {
                let ffmpeg_path = download::get_ffmpeg_path(&app_clone);
                let extraction_dir = match app_clone.path().temp_dir() {
                    Ok(dir) => dir,
                    Err(e) => {
                        if let Ok(db_lock) = db_clone.lock() {
                            let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                        }
                        let _ = app_clone.emit(
                            "transcription-progress",
                            serde_json::json!({
                                "id": id_clone,
                                "progress": 0.0,
                                "status": "error",
                                "error": e.to_string()
                            }),
                        );
                        return;
                    }
                };

                let extracted_audio = extraction_dir.join(format!("transcribe-{}-local.wav", id_clone));
                let source_input = audio_path.to_string_lossy().to_string();
                let extracted_output = extracted_audio.to_string_lossy().to_string();
                let ffmpeg_result = download::create_hidden_command(&ffmpeg_path)
                    .args([
                        "-y",
                        "-i",
                        &source_input,
                        "-vn",
                        "-ac",
                        "1",
                        "-ar",
                        "16000",
                        &extracted_output,
                    ])
                    .output()
                    .await;

                match ffmpeg_result {
                    Ok(result) if result.status.success() => {
                        local_audio_path = extracted_audio.clone();
                        temp_files.push(extracted_audio);
                    }
                    Ok(result) => {
                        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
                        let err = format!("Failed to extract audio from media file: {}", stderr.trim());
                        if let Ok(db_lock) = db_clone.lock() {
                            let _ = db_lock.update_transcript_error(&id_clone, &err);
                        }
                        let _ = app_clone.emit(
                            "transcription-progress",
                            serde_json::json!({
                                "id": id_clone,
                                "progress": 0.0,
                                "status": "error",
                                "error": err
                            }),
                        );
                        return;
                    }
                    Err(e) => {
                        let err = format!("Failed to run ffmpeg for local transcription: {}", e);
                        if let Ok(db_lock) = db_clone.lock() {
                            let _ = db_lock.update_transcript_error(&id_clone, &err);
                        }
                        let _ = app_clone.emit(
                            "transcription-progress",
                            serde_json::json!({
                                "id": id_clone,
                                "progress": 0.0,
                                "status": "error",
                                "error": err
                            }),
                        );
                        return;
                    }
                }
            }

            let temp_dir = match app_clone.path().temp_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    if let Ok(db_lock) = db_clone.lock() {
                        let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                    }
                    let _ = app_clone.emit(
                        "transcription-progress",
                        serde_json::json!({
                            "id": id_clone,
                            "progress": 0.0,
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                    return;
                }
            };

            let output_base = temp_dir.join(format!("transcribe-{}", id_clone));
            let output_txt = output_base.with_extension("txt");

            let audio_path_str = local_audio_path.to_string_lossy().to_string();
            let output_base_str = output_base.to_string_lossy().to_string();
            let mut cmd = download::create_hidden_command(&whisper_cpp_clone);
            cmd.args([
                "-m",
                &whisper_model_clone,
                "-f",
                &audio_path_str,
                "-otxt",
                "-of",
                &output_base_str,
            ])
            .stdin(Stdio::null());

            let mut child = match cmd.spawn() {
                Ok(child) => child,
                Err(e) => {
                    if let Ok(db_lock) = db_clone.lock() {
                        let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                    }
                    let _ = app_clone.emit(
                        "transcription-progress",
                        serde_json::json!({
                            "id": id_clone,
                            "progress": 0.0,
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                    return;
                }
            };

            let status = tokio::select! {
                result = child.wait() => {
                    match result {
                        Ok(status) => Some(status),
                        Err(e) => {
                            if let Ok(db_lock) = db_clone.lock() {
                                let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                            }
                            let _ = app_clone.emit(
                                "transcription-progress",
                                serde_json::json!({
                                    "id": id_clone,
                                    "progress": 0.0,
                                    "status": "error",
                                    "error": e.to_string()
                                }),
                            );
                            return;
                        }
                    }
                }
                _ = wait_for_cancel(cancel_rx_clone.clone()) => {
                    let _ = child.kill().await;
                    None
                }
            };

            let Some(status) = status else {
                return;
            };

            if !status.success() {
                let err = format!("whisper.cpp exited with status {}", status);
                if let Ok(db_lock) = db_clone.lock() {
                    let _ = db_lock.update_transcript_error(&id_clone, &err);
                }
                let _ = app_clone.emit(
                    "transcription-progress",
                    serde_json::json!({
                        "id": id_clone,
                        "progress": 0.0,
                        "status": "error",
                        "error": err
                    }),
                );
                return;
            }

            let text = match tokio::fs::read_to_string(&output_txt).await {
                Ok(t) => t,
                Err(e) => {
                    if let Ok(db_lock) = db_clone.lock() {
                        let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                    }
                    let _ = app_clone.emit(
                        "transcription-progress",
                        serde_json::json!({
                            "id": id_clone,
                            "progress": 0.0,
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                    return;
                }
            };

            (text, String::new())
        } else {
            let api_key = if !api_key_clone.is_empty() {
                api_key_clone
            } else {
                let err = "OpenAI API key is missing";
                if let Ok(db_lock) = db_clone.lock() {
                    let _ = db_lock.update_transcript_error(&id_clone, err);
                }
                let _ = app_clone.emit(
                    "transcription-progress",
                    serde_json::json!({
                        "id": id_clone,
                        "progress": 0.0,
                        "status": "error",
                        "error": err
                    }),
                );
                return;
            };

            let bytes = match tokio::fs::read(&audio_path).await {
                Ok(b) => b,
                Err(e) => {
                    if let Ok(db_lock) = db_clone.lock() {
                        let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                    }
                    let _ = app_clone.emit(
                        "transcription-progress",
                        serde_json::json!({
                            "id": id_clone,
                            "progress": 0.0,
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                    return;
                }
            };

            let model = if !model_override_clone.is_empty() {
                model_override_clone
            } else {
                api_model_clone
            };

            let part = reqwest::multipart::Part::bytes(bytes).file_name("audio.mp3");
            let form = reqwest::multipart::Form::new()
                .text("model", model)
                .part("file", part);

            let client = reqwest::Client::new();
            let response = match client
                .post("https://api.openai.com/v1/audio/transcriptions")
                .bearer_auth(api_key)
                .multipart(form)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if let Ok(db_lock) = db_clone.lock() {
                        let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                    }
                    let _ = app_clone.emit(
                        "transcription-progress",
                        serde_json::json!({
                            "id": id_clone,
                            "progress": 0.0,
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                    return;
                }
            };

            if !response.status().is_success() {
                let body = response.text().await.unwrap_or_default();
                if let Ok(db_lock) = db_clone.lock() {
                    let _ = db_lock.update_transcript_error(&id_clone, &body);
                }
                let _ = app_clone.emit(
                    "transcription-progress",
                    serde_json::json!({
                        "id": id_clone,
                        "progress": 0.0,
                        "status": "error",
                        "error": body
                    }),
                );
                return;
            }

            let json: serde_json::Value = match response.json().await {
                Ok(v) => v,
                Err(e) => {
                    if let Ok(db_lock) = db_clone.lock() {
                        let _ = db_lock.update_transcript_error(&id_clone, &e.to_string());
                    }
                    let _ = app_clone.emit(
                        "transcription-progress",
                        serde_json::json!({
                            "id": id_clone,
                            "progress": 0.0,
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                    return;
                }
            };

            let text = json["text"].as_str().unwrap_or("").to_string();
            let language = json["language"].as_str().unwrap_or("").to_string();
            (text, language)
        };

        if let Ok(db_lock) = db_clone.lock() {
            let _ = db_lock.update_transcript_complete(&id_clone, &text, &language);
        }

        let _ = app_clone.emit(
            "transcription-progress",
            serde_json::json!({
                "id": id_clone,
                "progress": 100.0,
                "status": "completed",
                "text": text,
                "language": language
            }),
        );

        for temp in temp_files {
            let _ = tokio::fs::remove_file(temp).await;
        }

        };

        run.await;

        let mut jobs = transcription_jobs_clone.lock().await;
        jobs.remove(&id_clone);
    });

    Ok(id)
}

#[tauri::command]
pub async fn get_transcripts(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<serde_json::Value>, String> {
    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock.get_transcripts().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_transcript(
    db: State<'_, Arc<Mutex<Database>>>,
    transcription_jobs: State<'_, Arc<tokio::sync::Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>>>,
    id: String,
) -> Result<(), String> {
    {
        let mut jobs = transcription_jobs.lock().await;
        if let Some(cancel) = jobs.remove(&id) {
            let _ = cancel.send(true);
        }
    }

    let db_lock = db.lock().map_err(|e| e.to_string())?;
    db_lock.delete_transcript(&id).map_err(|e| e.to_string())
}

fn map_local_model_to_filename(model_id: &str) -> Result<&'static str, String> {
    match model_id {
        "whisper-tiny" => Ok("ggml-tiny.bin"),
        "whisper-base" => Ok("ggml-base.bin"),
        "whisper-small" => Ok("ggml-small.bin"),
        "whisper-medium" => Ok("ggml-medium.bin"),
        "whisper-large-v3" => Ok("ggml-large-v3.bin"),
        "distil-whisper-large-v3" => Ok("ggml-large-v3-turbo.bin"),
        "faster-whisper-large-v3" | "vosk-small" => Err(format!(
            "Model '{}' is not supported by whisper.cpp backend yet. Choose a whisper.cpp model.",
            model_id
        )),
        _ => Err(format!("Unknown local model id: {}", model_id)),
    }
}

#[tauri::command]
pub async fn check_openai_transcription_api(
    api_key: String,
    model: String,
) -> Result<serde_json::Value, String> {
    if api_key.trim().is_empty() {
        return Err("OpenAI API key is missing".to_string());
    }

    let model_name = if model.trim().is_empty() {
        "whisper-1".to_string()
    } else {
        model.trim().to_string()
    };

    let client = reqwest::Client::builder()
        .user_agent("YTDL/3.0")
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(format!("https://api.openai.com/v1/models/{}", model_name))
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API check failed ({}): {}", status, body));
    }

    Ok(serde_json::json!({
        "ok": true,
        "model": model_name
    }))
}

#[tauri::command]
pub async fn install_local_transcription(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    model_id: String,
) -> Result<serde_json::Value, String> {
    let model_filename = map_local_model_to_filename(&model_id)?;

    if !cfg!(target_os = "windows") {
        return Err("Automatic local transcription install is currently supported on Windows only".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    let whisper_root = app_data_dir.join("whisper");
    let bin_dir = whisper_root.join("bin");
    let model_dir = whisper_root.join("models");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let whisper_cli = bin_dir.join("whisper-cli.exe");

    let client = reqwest::Client::builder()
        .user_agent("YTDL/3.0")
        .build()
        .map_err(|e| e.to_string())?;

    if !whisper_cli.exists() {
        let _ = app.emit("install-progress", serde_json::json!({
            "tool": "whisper.cpp",
            "status": "downloading",
            "progress": 10
        }));

        let release_json: serde_json::Value = client
            .get("https://api.github.com/repos/ggml-org/whisper.cpp/releases/latest")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch whisper.cpp release info: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse whisper.cpp release info: {}", e))?;

        let assets = release_json["assets"]
            .as_array()
            .ok_or_else(|| "whisper.cpp release assets are missing".to_string())?;

        let asset_name = if cfg!(target_arch = "x86") {
            "whisper-bin-Win32.zip"
        } else {
            "whisper-bin-x64.zip"
        };

        let asset_url = assets
            .iter()
            .find(|a| a["name"].as_str() == Some(asset_name))
            .and_then(|a| a["browser_download_url"].as_str())
            .ok_or_else(|| format!("Could not find '{}' in whisper.cpp latest release", asset_name))?;

        let zip_bytes = client
            .get(asset_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download whisper.cpp binaries: {}", e))?
            .bytes()
            .await
            .map_err(|e| format!("Failed to read whisper.cpp archive: {}", e))?;

        let temp_zip = whisper_root.join("whisper-bin-temp.zip");
        std::fs::write(&temp_zip, &zip_bytes)
            .map_err(|e| format!("Failed to write whisper.cpp temp archive: {}", e))?;

        let file = std::fs::File::open(&temp_zip)
            .map_err(|e| format!("Failed to open whisper.cpp archive: {}", e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Failed to parse whisper.cpp archive: {}", e))?;

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read archive entry: {}", e))?;

            if entry.is_dir() {
                continue;
            }

            let entry_name = entry.name().replace('\\', "/");
            let Some(filename) = Path::new(&entry_name).file_name() else {
                continue;
            };

            let filename_str = filename.to_string_lossy().to_string();
            let is_runtime_file = filename_str.ends_with(".exe") || filename_str.ends_with(".dll");
            if !is_runtime_file {
                continue;
            }

            let dest = bin_dir.join(&filename_str);
            let mut outfile = std::fs::File::create(&dest)
                .map_err(|e| format!("Failed to create runtime file '{}': {}", filename_str, e))?;
            std::io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("Failed to extract '{}': {}", filename_str, e))?;
        }

        let main_legacy = bin_dir.join("main.exe");
        if !whisper_cli.exists() && main_legacy.exists() {
            std::fs::rename(&main_legacy, &whisper_cli)
                .map_err(|e| format!("Failed to rename main.exe to whisper-cli.exe: {}", e))?;
        }

        let _ = std::fs::remove_file(&temp_zip);

        if !whisper_cli.exists() {
            return Err("whisper-cli.exe was not found in downloaded whisper.cpp binaries".to_string());
        }
    }

    let model_path = model_dir.join(model_filename);
    if !model_path.exists() {
        let _ = app.emit("install-progress", serde_json::json!({
            "tool": "whisper-model",
            "status": "downloading",
            "progress": 60
        }));

        let model_url = format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}?download=true",
            model_filename
        );
        let model_bytes = client
            .get(&model_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download model '{}': {}", model_filename, e))?
            .bytes()
            .await
            .map_err(|e| format!("Failed to read model '{}': {}", model_filename, e))?;

        std::fs::write(&model_path, &model_bytes)
            .map_err(|e| format!("Failed to save model '{}': {}", model_filename, e))?;
    }

    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock
            .save_setting("whisper_cpp_path", &whisper_cli.to_string_lossy())
            .map_err(|e| e.to_string())?;
        db_lock
            .save_setting("whisper_model_path", &model_path.to_string_lossy())
            .map_err(|e| e.to_string())?;
        db_lock
            .save_setting("local_model_id", &model_id)
            .map_err(|e| e.to_string())?;
        db_lock
            .save_setting("transcribe_provider", "local")
            .map_err(|e| e.to_string())?;
        db_lock
            .save_setting("transcription_configured", "true")
            .map_err(|e| e.to_string())?;
    }

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "whisper-local",
        "status": "completed",
        "progress": 100
    }));

    Ok(serde_json::json!({
        "ok": true,
        "modelId": model_id,
        "whisperCppPath": whisper_cli,
        "whisperModelPath": model_path,
    }))
}


//  Tool checks 

#[tauri::command]
pub async fn check_ytdlp(app: AppHandle) -> Result<bool, String> {
    let ytdlp = download::get_ytdlp_path(&app);
    let result = download::create_hidden_command(&ytdlp)
        .arg("--version")
        .output()
        .await;
    Ok(result.map(|o| o.status.success()).unwrap_or(false))
}

#[tauri::command]
pub async fn check_ffmpeg(app: AppHandle) -> Result<bool, String> {
    let ffmpeg = download::get_ffmpeg_path(&app);
    let result = download::create_hidden_command(&ffmpeg)
        .arg("-version")
        .output()
        .await;
    Ok(result.map(|o| o.status.success()).unwrap_or(false))
}

/// Install yt-dlp binary from GitHub releases.
#[tauri::command]
pub async fn install_ytdlp(app: AppHandle) -> Result<(), String> {
    let bin_dir = ensure_tool_bin_dir(&app)?;

    let (url, filename) = if cfg!(target_os = "windows") {
        ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe", "yt-dlp.exe")
    } else if cfg!(target_os = "android") {
        if cfg!(target_arch = "aarch64") {
            ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64", "yt-dlp")
        } else if cfg!(target_arch = "x86_64") {
            ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux", "yt-dlp")
        } else {
            return Err("Android auto-install currently supports only aarch64 and x86_64 targets".to_string());
        }
    } else if cfg!(target_os = "macos") {
        ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos", "yt-dlp")
    } else {
        ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp", "yt-dlp")
    };

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "yt-dlp",
        "status": "downloading",
        "progress": 0
    }));

    let response = reqwest::get(url).await.map_err(|e| format!("Download failed: {}", e))?;
    let bytes = response.bytes().await.map_err(|e| format!("Read failed: {}", e))?;

    let dest = bin_dir.join(filename);
    std::fs::write(&dest, &bytes).map_err(|e| format!("Write failed: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod failed: {}", e))?;
    }

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "yt-dlp",
        "status": "completed",
        "progress": 100
    }));

    Ok(())
}

/// Install ffmpeg binary.
#[tauri::command]
pub async fn install_ffmpeg(app: AppHandle) -> Result<(), String> {
    let bin_dir = ensure_tool_bin_dir(&app)?;

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "ffmpeg",
        "status": "downloading",
        "progress": 0
    }));

    if cfg!(target_os = "windows") {
        use std::io::{Read, Write};

        // Download ffmpeg ZIP
        let url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
        let response = reqwest::get(url).await.map_err(|e| format!("Download failed: {}", e))?;
        let bytes = response.bytes().await.map_err(|e| format!("Read failed: {}", e))?;

        let _ = app.emit("install-progress", serde_json::json!({
            "tool": "ffmpeg",
            "status": "extracting",
            "progress": 50
        }));

        // Write to temp zip file
        let temp_zip = bin_dir.join("ffmpeg_temp.zip");
        std::fs::write(&temp_zip, &bytes).map_err(|e| format!("Write ZIP failed: {}", e))?;

        // Extract ffmpeg.exe from ZIP
        let file = std::fs::File::open(&temp_zip).map_err(|e| format!("Open ZIP failed: {}", e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Parse ZIP failed: {}", e))?;

        let mut found = false;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| format!("Read entry failed: {}", e))?;
            let name = entry.name().to_lowercase();
            
            // Find ffmpeg.exe in the archive (it's in a subdirectory)
            if name.ends_with("bin/ffmpeg.exe") || name.ends_with("bin\\ffmpeg.exe") {
                let dest = bin_dir.join("ffmpeg.exe");
                let mut outfile = std::fs::File::create(&dest).map_err(|e| format!("Create failed: {}", e))?;
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer).map_err(|e| format!("Read failed: {}", e))?;
                outfile.write_all(&buffer).map_err(|e| format!("Write failed: {}", e))?;
                found = true;
            }
            // Also extract ffprobe.exe if present
            if name.ends_with("bin/ffprobe.exe") || name.ends_with("bin\\ffprobe.exe") {
                let dest = bin_dir.join("ffprobe.exe");
                let mut outfile = std::fs::File::create(&dest).map_err(|e| format!("Create failed: {}", e))?;
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer).map_err(|e| format!("Read failed: {}", e))?;
                outfile.write_all(&buffer).map_err(|e| format!("Write failed: {}", e))?;
            }
        }

        // Clean up temp zip
        let _ = std::fs::remove_file(&temp_zip);

        if !found {
            return Err("Could not find ffmpeg.exe in ZIP archive".to_string());
        }
    } else {
        let (ffmpeg_url, ffprobe_url) = if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                (
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-darwin-arm64",
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-darwin-arm64",
                )
            } else {
                (
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-darwin-x64",
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-darwin-x64",
                )
            }
        } else if cfg!(target_os = "android") {
            if cfg!(target_arch = "aarch64") {
                (
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-linux-arm64",
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-linux-arm64",
                )
            } else if cfg!(target_arch = "x86_64") {
                (
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-linux-x64",
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-linux-x64",
                )
            } else if cfg!(target_arch = "arm") {
                (
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-linux-arm",
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-linux-arm",
                )
            } else {
                return Err("Android auto-install currently supports only arm/arm64/x86_64 targets".to_string());
            }
        } else {
            if cfg!(target_arch = "aarch64") {
                (
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-linux-arm64",
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-linux-arm64",
                )
            } else if cfg!(target_arch = "arm") {
                (
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-linux-arm",
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-linux-arm",
                )
            } else {
                (
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-linux-x64",
                    "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-linux-x64",
                )
            }
        };

        let ffmpeg_dest = bin_dir.join("ffmpeg");
        let ffprobe_dest = bin_dir.join("ffprobe");

        let ffmpeg_bytes = reqwest::get(ffmpeg_url)
            .await
            .map_err(|e| format!("Download failed: {}", e))?
            .bytes()
            .await
            .map_err(|e| format!("Read failed: {}", e))?;
        std::fs::write(&ffmpeg_dest, &ffmpeg_bytes).map_err(|e| format!("Write failed: {}", e))?;

        let _ = app.emit("install-progress", serde_json::json!({
            "tool": "ffmpeg",
            "status": "downloading",
            "progress": 75
        }));

        let ffprobe_bytes = reqwest::get(ffprobe_url)
            .await
            .map_err(|e| format!("Download failed: {}", e))?
            .bytes()
            .await
            .map_err(|e| format!("Read failed: {}", e))?;
        std::fs::write(&ffprobe_dest, &ffprobe_bytes).map_err(|e| format!("Write failed: {}", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ffmpeg_dest, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("chmod failed: {}", e))?;
            std::fs::set_permissions(&ffprobe_dest, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("chmod failed: {}", e))?;
        }
    }

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "ffmpeg",
        "status": "completed",
        "progress": 100
    }));

    Ok(())
}


#[tauri::command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get currently installed yt-dlp version
#[tauri::command]
pub async fn get_ytdlp_version(app: AppHandle) -> Result<String, String> {
    let ytdlp = download::get_ytdlp_path(&app);
    let output = download::create_hidden_command(&ytdlp)
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("Failed to get version: {}", e))?;
    
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(version)
    } else {
        Err("yt-dlp not installed".to_string())
    }
}

/// Get latest available yt-dlp version from GitHub
#[tauri::command]
pub async fn get_ytdlp_latest_version() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("YTDL/3.0")
        .build()
        .map_err(|e| e.to_string())?;
    
    let response = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .send()
        .await
        .map_err(|e| format!("Failed to check for updates: {}", e))?;
    
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    json["tag_name"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Could not find version".to_string())
}

/// Update yt-dlp to latest version
#[tauri::command]
pub async fn update_ytdlp(app: AppHandle) -> Result<(), String> {
    // Use the same function as install
    install_ytdlp(app).await
}

/// Get currently installed ffmpeg version
#[tauri::command]
pub async fn get_ffmpeg_version(app: AppHandle) -> Result<String, String> {
    let ffmpeg = download::get_ffmpeg_path(&app);
    let output = download::create_hidden_command(&ffmpeg)
        .arg("-version")
        .output()
        .await
        .map_err(|e| format!("Failed to get version: {}", e))?;
    
    if output.status.success() {
        let full_output = String::from_utf8_lossy(&output.stdout);
        // Extract version from first line: "ffmpeg version N-xxxxx-..."
        if let Some(first_line) = full_output.lines().next() {
            if let Some(version_part) = first_line.strip_prefix("ffmpeg version ") {
                let version = version_part.split_whitespace().next().unwrap_or(version_part);
                return Ok(version.to_string());
            }
        }
        Ok(full_output.lines().next().unwrap_or("unknown").to_string())
    } else {
        Err("ffmpeg not installed".to_string())
    }
}

/// Check if there's a newer ffmpeg version available
/// Note: This is a simplified check since ffmpeg doesn't have a simple API
#[tauri::command]
pub async fn check_ffmpeg_update() -> Result<bool, String> {
    // For ffmpeg, we'll just return false for now since checking for updates
    // is more complex (no simple API like GitHub releases)
    // In production, you might want to check the actual website or use a date-based check
    Ok(false)
}

/// Update ffmpeg to latest version
#[tauri::command]
pub async fn update_ffmpeg(app: AppHandle) -> Result<(), String> {
    // Use the same function as install
    install_ffmpeg(app).await
}

#[tauri::command]
pub async fn open_external(url: String) -> Result<(), String> {
    // Only allow http/https URLs
    let trimmed = url.trim().to_lowercase();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Only http/https URLs are allowed".to_string());
    }
    open::that(&url).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        // On Windows, use cmd.exe with 'start' to handle special characters properly
        use std::process::Command;
        Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn()
            .map_err(|e| format!("Failed to open '{}': {}", path, e))?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        open::that(&path).map_err(|e| e.to_string())
    }
}

// ────────────────────────────────── Stream Proxy (Custom Player) ──────────────────────────────────

/// Extract direct stream URLs from a video URL using yt-dlp.
/// This allows playing videos in a custom player even in countries where YouTube is blocked,
/// because yt-dlp can use proxies/cookies and returns direct CDN URLs.
#[tauri::command]
pub async fn get_stream_url(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    url: String,
) -> Result<serde_json::Value, String> {
    validate_url(&url)?;

    let ytdlp = download::get_ytdlp_path(&app);

    // Get browser cookies setting for bypassing restrictions
    let browser_cookies = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
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
        args.insert(0, format!("--cookies-from-browser"));
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

        let (title, items) = match rss::fetch_feed_items(&normalized_url).await {
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

// ────────────────────────────────────────────────── Transcription ──────────────────────────────────────────────────

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

// ────────────────────────────────── Batch Download Operations ──────────────────────────────────

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
        .filter(|d| d["status"].as_str() == Some("downloading"))
        .filter_map(|d| d["id"].as_str().map(String::from))
        .collect();

    let mut paused_count = 0u32;
    for id in active_ids {
        let mut dl_lock = dl.lock().await;
        if dl_lock.pause(&id) {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            let _ = db_lock.update_download_status(&id, "paused");
            paused_count += 1;
        }
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
    for id in paused_ids {
        let mut dl_lock = dl.lock().await;
        if dl_lock.resume(&id) {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            let _ = db_lock.update_download_status(&id, "downloading");
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
        let mut dl_lock = dl.lock().await;
        dl_lock.cancel(&id);
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let _ = db_lock.update_download_status(&id, "cancelled");
        cancelled_count += 1;
    }

    Ok(cancelled_count)
}