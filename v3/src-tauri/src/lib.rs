pub mod commands;
pub mod db;
pub mod download;
pub mod error;
pub mod playlist_commands;
pub mod rss;
pub mod rss_scheduler;
pub mod settings;
pub mod transcription_commands;
pub mod tool_install_commands;
pub mod android_commands;
#[cfg(target_os = "android")]
pub mod android_bridge;

use std::collections::HashMap;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging backend.
    // On Android: android_logger sends log::* output to logcat.
    // On desktop: env_logger sends log::* output to stderr.
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("YTDL-Rust"),
        );
        log::info!("[YTDL] Android logger initialized — Rust logs now visible in logcat");
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = env_logger::try_init();
    }

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
            log::info!("[YTDL] setup() starting...");

            let app_data = app
                .path()
                .app_data_dir()
                .map_err(|e| {
                    log::error!("[YTDL] Failed to resolve app data dir: {}", e);
                    format!("Failed to resolve app data dir: {}", e)
                })?;
            
            log::info!("[YTDL] app_data_dir resolved: {}", app_data.display());

            // Try to create app data directory
            if let Err(e) = std::fs::create_dir_all(&app_data) {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    log::error!("[YTDL] Permission denied creating app data dir: {}", app_data.display());
                    return Err(format!(
                        "Permission denied when creating app data directory '{}'. Please check app permissions.",
                        app_data.display()
                    ).into());
                }
                log::error!("[YTDL] Failed to create app data dir: {}", e);
                return Err(format!("Failed to create app data directory: {}", e).into());
            }

            log::info!("App data directory: {}", app_data.display());

            // Initialize database
            let db_path = app_data.join("ytdl.db");
            log::info!("[YTDL] Opening database at: {}", db_path.display());
            let database = db::Database::new(&db_path).map_err(|e| {
                log::error!("[YTDL] Database open failed: {}", e);
                e
            })?;
            database.migrate().map_err(|e| {
                log::error!("[YTDL] Database migration failed: {}", e);
                e
            })?;
            log::info!("[YTDL] Database ready");

            #[cfg(any(target_os = "android", target_os = "ios"))]
            {
                // On Android, Termux downloads to shared storage (/sdcard/Download/YTDL).
                // The default download_path MUST point there so Termux can write files
                // and our app can read them. Using app_data_dir would be inaccessible
                // to Termux due to Android's per-app data isolation.
                #[cfg(target_os = "android")]
                let mobile_download_dir = std::path::PathBuf::from(
                    download::android_shared_download_dir()
                );
                #[cfg(not(target_os = "android"))]
                let mobile_download_dir = app_data.join("downloads").join("YTDL");

                match std::fs::create_dir_all(&mobile_download_dir) {
                    Ok(()) => {
                        if database.get_setting("download_path")?.is_none() {
                            database.save_setting(
                                "download_path",
                                &mobile_download_dir.to_string_lossy(),
                            )?;
                        }
                        log::info!("Mobile download directory: {}", mobile_download_dir.display());
                    }
                    Err(e) => {
                        log::warn!("Failed to create mobile download directory '{}': {}. Using fallback.", mobile_download_dir.display(), e);
                        // Fallback: store the desired path anyway — Termux mkdir will
                        // create it when the first download runs.
                        if database.get_setting("download_path")?.is_none() {
                            database.save_setting(
                                "download_path",
                                &mobile_download_dir.to_string_lossy(),
                            )?;
                        }
                    }
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
            transcription_commands::start_transcription,
            transcription_commands::get_transcripts,
            transcription_commands::delete_transcript,
            transcription_commands::check_openai_transcription_api,
            transcription_commands::install_local_transcription,
            tool_install_commands::check_ytdlp,
            tool_install_commands::check_ffmpeg,
            tool_install_commands::install_ytdlp,
            tool_install_commands::install_ffmpeg,
            tool_install_commands::get_ytdlp_version,
            tool_install_commands::get_ytdlp_latest_version,
            tool_install_commands::update_ytdlp,
            tool_install_commands::get_ffmpeg_version,
            tool_install_commands::check_ffmpeg_update,
            tool_install_commands::update_ffmpeg,
            commands::get_platform,
            tool_install_commands::get_app_version,
            tool_install_commands::get_binary_info,
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
            // Android / Termux
            android_commands::get_android_info,
            android_commands::open_termux,
            android_commands::open_termux_install_page,
            android_commands::launch_termux_setup,
            android_commands::termux_download,
            android_commands::request_storage_permission,
            tool_install_commands::probe_ytdlp,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            log::error!("Fatal: failed to start YTDL: {}", e);
            eprintln!("Error while running YTDL: {}", e);
            // IMPORTANT: Do NOT call std::process::exit() on Android!
            // It kills the JVM process silently (SIGKILL in logcat, no Java trace).
            // A panic! at least unwinds the stack and produces a visible crash trace
            // that the Kotlin UncaughtExceptionHandler can capture.
            panic!("YTDL failed to start: {}", e);
        });
}
