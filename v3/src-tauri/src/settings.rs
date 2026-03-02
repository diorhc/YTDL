/// Typed representation of application settings.
///
/// Currently settings are persisted as individual key-value pairs in the
/// database via `save_setting` / `get_settings` commands.  This struct is
/// kept for reference and potential future migration to a strongly-typed
/// config layer.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub download_path: String,
    pub notifications: bool,
    pub close_to_tray: bool,
    pub auto_launch: bool,
    pub auto_start_download: bool,
    pub embed_thumbnail: bool,
    pub embed_metadata: bool,
    pub browser_cookies: String,
    pub ytdlp_flags: String,
    pub config_file: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        // On Android, default to the shared storage path that Termux can access
        #[cfg(target_os = "android")]
        let download_dir = {
            let base = std::env::var("EXTERNAL_STORAGE").unwrap_or_else(|_| "/sdcard".to_string());
            std::path::PathBuf::from(format!("{}/Download/YTDL", base))
        };
        
        #[cfg(not(target_os = "android"))]
        let download_dir = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
            .unwrap_or_else(|| std::path::PathBuf::from("YTDL"))
            .join("YTDL");

        Self {
            theme: "system".to_string(),
            language: "en".to_string(),
            download_path: download_dir.to_string_lossy().to_string(),
            notifications: true,
            close_to_tray: false,
            auto_launch: false,
            auto_start_download: true,
            embed_thumbnail: true,
            embed_metadata: true,
            browser_cookies: "none".to_string(),
            ytdlp_flags: String::new(),
            config_file: String::new(),
        }
    }
}
