pub mod commands;
pub mod db;
pub mod download;
pub mod error;
pub mod logger;
pub mod playlist_commands;
pub mod rss;
pub mod rss_scheduler;
pub mod settings;

use std::collections::HashMap;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::default().build());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder = builder;

    builder
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data dir");
            std::fs::create_dir_all(&app_data)
                .map_err(|e| format!("Failed to create app data directory: {}", e))?;

            // Initialize database
            let db_path = app_data.join("ytdl.db");
            let database = db::Database::new(&db_path)?;
            database.migrate()?;

            #[cfg(any(target_os = "android", target_os = "ios"))]
            {
                let mobile_download_dir = app_data.join("downloads").join("YTDL");
                std::fs::create_dir_all(&mobile_download_dir)
                    .map_err(|e| format!("Failed to create mobile download directory: {}", e))?;
                if database.get_setting("download_path")?.is_none() {
                    database.save_setting(
                        "download_path",
                        &mobile_download_dir.to_string_lossy(),
                    )?;
                }
            }

            app.manage(std::sync::Arc::new(std::sync::Mutex::new(database)));

            // Initialize download manager
            let download_mgr = download::DownloadManager::new();
            app.manage(std::sync::Arc::new(tokio::sync::Mutex::new(download_mgr)));

            // Initialize RSS scheduler
            let rss_scheduler = rss_scheduler::RssScheduler::new();
            app.manage(std::sync::Arc::new(tokio::sync::Mutex::new(rss_scheduler)));

            // Initialize active transcription cancellation tokens
            let transcription_jobs: std::sync::Arc<
                tokio::sync::Mutex<
                    HashMap<String, tokio::sync::watch::Sender<bool>>,
                >,
            > = std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));
            app.manage(transcription_jobs);

            // Start RSS scheduler in background
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let scheduler = app_handle.state::<std::sync::Arc<tokio::sync::Mutex<rss_scheduler::RssScheduler>>>();
                let scheduler = scheduler.lock().await;
                scheduler.start(app_handle.clone()).await;
            });

            log::info!("YTDL v{} started", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_video_info,
            playlist_commands::get_playlist_info,
            commands::start_download,
            playlist_commands::start_playlist_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::retry_download,
            commands::delete_download,
            commands::get_downloads,
            commands::get_settings,
            commands::save_setting,
            commands::select_directory,
            commands::get_feeds,
            commands::add_feed,
            commands::remove_feed,
            commands::check_feed,
            commands::start_transcription,
            commands::get_transcripts,
            commands::delete_transcript,
            commands::check_openai_transcription_api,
            commands::install_local_transcription,
            commands::check_ytdlp,
            commands::check_ffmpeg,
            commands::install_ytdlp,
            commands::install_ffmpeg,
            commands::get_ytdlp_version,
            commands::get_ytdlp_latest_version,
            commands::update_ytdlp,
            commands::get_ffmpeg_version,
            commands::check_ffmpeg_update,
            commands::update_ffmpeg,
            commands::get_platform,
            commands::get_app_version,
            commands::open_external,
            commands::open_path,
            // RSS Scheduler
            commands::set_rss_check_interval,
            commands::get_rss_check_interval,
            commands::check_all_rss_feeds,
            commands::mark_feed_item_watched,
            commands::update_feed_settings,
            // Stream proxy
            commands::get_stream_url,
            // Batch operations
            commands::pause_all_downloads,
            commands::resume_all_downloads,
            commands::cancel_all_downloads,
            commands::set_download_priority,
            // Export
            commands::export_downloads,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running YTDL");
}
