use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

use crate::commands::validate_url;
use crate::db::Database;
use crate::download::{self, DownloadManager};

#[tauri::command]
pub async fn get_playlist_info(app: AppHandle, url: String) -> Result<serde_json::Value, String> {
    validate_url(&url)?;
    let ytdlp = download::get_ytdlp_path(&app);
    log::info!("Fetching playlist info for: {}", url);
    let info = download::fetch_playlist_info(&ytdlp, &url)
        .await
        .map_err(|e| {
            log::error!("Playlist fetch error: {}", e);
            e.to_string()
        })?;
    log::info!("Playlist fetched: {} entries", info.entry_count);
    serde_json::to_value(&info).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_playlist_download(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    dl: State<'_, Arc<tokio::sync::Mutex<DownloadManager>>>,
    url: String,
    start_index: Option<usize>,
    end_index: Option<usize>,
    format: Option<String>,
) -> Result<Vec<String>, String> {
    validate_url(&url)?;
    let ytdlp = download::get_ytdlp_path(&app);
    let playlist_info = download::fetch_playlist_info(&ytdlp, &url)
        .await
        .map_err(|e| e.to_string())?;

    let start = start_index.unwrap_or(1).max(1);
    let end = end_index
        .unwrap_or(playlist_info.entry_count)
        .min(playlist_info.entry_count);

    if start > end || start < 1 {
        return Err("Invalid playlist range".to_string());
    }

    let mut download_ids = Vec::new();
    let mut entries_to_start: Vec<(String, String)> = Vec::new();

    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;

        for entry in playlist_info.entries.iter() {
            if entry.index < start || entry.index > end {
                continue;
            }

            // O(1) indexed lookup instead of O(n) in-memory scan
            if db_lock.download_exists_by_url(&entry.url, "").unwrap_or(None).is_some() {
                continue;
            }

            let id = uuid::Uuid::new_v4().to_string();
            let thumb = entry.thumbnail.clone().unwrap_or_default();
            db_lock
                .insert_download_with_source(&id, &entry.url, &entry.title, &thumb, "playlist")
                .map_err(|e| e.to_string())?;
            db_lock
                .update_download_status(&id, "queued")
                .map_err(|e| e.to_string())?;

            download_ids.push(id.clone());
            entries_to_start.push((id, entry.url.clone()));
        }
    }

    // Limit concurrent playlist downloads to avoid spawning hundreds of yt-dlp processes
    let concurrency = 3usize;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));

    for (id, url) in entries_to_start {
        let app_clone = app.clone();
        let db_clone = db.inner().clone();
        let dl_clone = dl.inner().clone();
        let format_clone = format.clone();
        let sem = semaphore.clone();
        tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let _ = crate::commands::start_download_existing(
                app_clone,
                db_clone,
                dl_clone,
                id,
                url,
                format_clone,
            )
            .await;
        });
    }

    Ok(download_ids)
}
