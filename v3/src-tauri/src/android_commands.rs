use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

use crate::db::Database;
#[cfg(target_os = "android")]
use crate::download;
use crate::commands::validate_url;
#[cfg(target_os = "android")]
use crate::commands::sanitize_ytdlp_flags;

/// Returns Termux availability information for the frontend to display
/// appropriate setup instructions.  On non-Android platforms returns a
/// default "no Termux" response so the frontend always gets a consistent shape.
#[tauri::command]
pub async fn get_android_info() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "android")]
    {
        let (installed, has_permission) = crate::android_bridge::termux_info();
        let native_lib_dir = download::get_native_lib_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Check storage permission (MANAGE_EXTERNAL_STORAGE on Android 11+).
        // This must be fresh every time because the user may grant it in Settings
        // while the app is running.
        let has_storage_permission = crate::android_bridge::check_storage_permission()
            .unwrap_or(false);

        return Ok(serde_json::json!({
            "platform": "android",
            "termuxInstalled": installed,
            "termuxHasPermission": has_permission,
            "hasStoragePermission": has_storage_permission,
            "nativeLibDir": native_lib_dir,
            // These are always false on Android — kept for API compatibility
            "bundledYtdlpWorks": false,
            "bundledFfmpegWorks": false,
        }));
    }

    #[cfg(not(target_os = "android"))]
    Ok(serde_json::json!({
        "platform": std::env::consts::OS,
        "termuxInstalled": false,
        "termuxHasPermission": false,
        "hasStoragePermission": true,
        "nativeLibDir": "",
        "bundledYtdlpWorks": false,
        "bundledFfmpegWorks": false,
    }))
}

/// Open Termux via JNI bridge (actually launches the app now).
#[tauri::command]
pub async fn open_termux() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        log::info!("[open_termux] Opening Termux via JNI bridge");
        match crate::android_bridge::open_termux_app() {
            Ok(true) => return Ok(()),
            Ok(false) => return Err("Failed to open Termux — is it installed?".to_string()),
            Err(e) => return Err(format!("JNI bridge error: {}", e)),
        }
    }
    #[cfg(not(target_os = "android"))]
    Ok(())
}

/// Open the Termux install page in the device browser (F-Droid).
#[tauri::command]
pub async fn open_termux_install_page() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        log::info!("[open_termux_install_page] Opening F-Droid page");
    }
    // On non-Android this is a no-op; the frontend opens the URL via open_external
    Ok(())
}

/// Open Termux and run yt-dlp/ffmpeg install commands automatically.
#[tauri::command]
pub async fn launch_termux_setup() -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        log::info!("[launch_termux_setup] Launching Termux auto-install");
        return crate::android_bridge::launch_termux_setup()
            .map_err(|e| format!("Failed to launch Termux setup: {}", e));
    }
    #[cfg(not(target_os = "android"))]
    Ok(false)
}

/// Download a video via Termux RUN_COMMAND (Android only).
/// Opens Termux in foreground so the user sees yt-dlp progress.
#[tauri::command]
pub async fn termux_download(
    _app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    url: String,
    format_id: Option<String>,
) -> Result<String, String> {
    validate_url(&url)?;

    #[cfg(target_os = "android")]
    {
        let (installed, has_perm) = crate::android_bridge::termux_info();
        if !installed {
            return Err("Termux is not installed. Please install Termux from F-Droid.".to_string());
        }
        if !has_perm {
            return Err("Termux RUN_COMMAND permission not granted. \
                       In Termux, run: echo 'allow-external-apps=true' >> ~/.termux/termux.properties \
                       Then restart Termux.".to_string());
        }

        // Use shared storage dir (accessible by both Termux and our app)
        let output_dir = {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            db_lock
                .get_setting("download_path")
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| crate::download::android_shared_download_dir())
        };

        // For Termux: prefer shared-storage accessible path
        let termux_output = if output_dir.starts_with("/data/data/") || output_dir.starts_with("/data/user/") {
            // App private dir — Termux can't write here
            crate::download::android_shared_download_dir()
        } else {
            output_dir
        };

        let format = format_id.unwrap_or_else(|| "bestvideo+bestaudio/best".to_string());

        // Get extra args from settings
        let extra_args = {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            let flags_str = db_lock
                .get_setting("ytdlp_flags")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if flags_str.is_empty() {
                vec![]
            } else {
                flags_str.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>()
            }
        };

        let sanitized_args = sanitize_ytdlp_flags(&extra_args);

        // Generate ID before launching so we can pass it to Termux for sentinel file
        let id = uuid::Uuid::new_v4().to_string();

        match crate::android_bridge::run_termux_download(
            &url,
            &termux_output,
            &format,
            &sanitized_args,
            &id,
        ) {
            Ok(true) => {
                log::info!("[termux_download] Download launched in Termux: {}", url);
                // Store in DB as "downloading" so the UI shows it
                {
                    let db_lock = db.lock().map_err(|e| e.to_string())?;
                    let title = format!("Termux: {}", url.chars().take(60).collect::<String>());
                    let _ = db_lock.insert_download(&id, &url, &title, "");
                    let _ = db_lock.update_download_status(&id, "downloading");
                }

                // Spawn background poller to detect Termux download completion
                let app_clone = _app.clone();
                let db_ref = db.inner().clone();
                let poll_id = id.clone();
                let poll_output_dir = termux_output.clone();
                tokio::spawn(async move {
                    crate::commands::poll_termux_download_status_pub(
                        &app_clone,
                        &db_ref,
                        &poll_id,
                        &poll_output_dir,
                    ).await;
                });

                Ok(id)
            }
            Ok(false) => Err("Failed to send download command to Termux".to_string()),
            Err(e) => Err(format!("Termux download failed: {}", e)),
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (_app, db, url, format_id);
        Err("Termux download is only available on Android".to_string())
    }
}

/// Open the system Settings screen so the user can grant MANAGE_EXTERNAL_STORAGE.
/// On Android 11+, this opens the "All files access" toggle for our app.
/// On pre-Android 11, returns false (WRITE_EXTERNAL_STORAGE is a normal runtime
/// permission handled by the Activity).
#[tauri::command]
pub async fn request_storage_permission() -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        log::info!("[request_storage_permission] Opening system settings for MANAGE_EXTERNAL_STORAGE");
        return crate::android_bridge::request_storage_permission()
            .map_err(|e| format!("Failed to open storage permission settings: {}", e));
    }
    #[cfg(not(target_os = "android"))]
    Ok(true)
}
