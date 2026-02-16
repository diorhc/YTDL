use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;
use tauri::Manager;

use crate::error::{AppError, AppResult};

/// Resolves the binary directory for storing yt-dlp, ffmpeg, and whisper binaries.
/// Uses app_data_dir on all platforms to ensure a user-writable location.
/// On Linux desktop, resource_dir typically points to read-only installation directories
/// (e.g., /usr/lib/ytdl/ or /opt/ytdl/), which causes OS Error 13 (Permission Denied).
/// On Android, uses app_data_dir which is always writable.
pub fn get_binary_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    // Try app_data_dir first (writable on all platforms including Android)
    if let Ok(base_dir) = app_handle.path().app_data_dir() {
        return base_dir.join("binaries");
    }

    // Fallback: try cache_dir
    if let Ok(base_dir) = app_handle.path().cache_dir() {
        return base_dir.join("binaries");
    }

    // Last fallback: temp directory
    if let Ok(base_dir) = app_handle.path().temp_dir() {
        return base_dir.join("binaries");
    }

    // Absolute last resort - use current directory
    PathBuf::from("binaries")
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
    Command::new(program)
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
/// In development, fallback to PATH. In production, use sidecar.
pub fn get_ytdlp_path(app_handle: &tauri::AppHandle) -> String {
    let bin_name: &str = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
    let sidecar: PathBuf = get_binary_dir(app_handle).join(bin_name);
    if sidecar.exists() {
        return sidecar.to_string_lossy().to_string();
    }
    if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" }.to_string()
}

/// Resolves the ffmpeg binary path.
pub fn get_ffmpeg_path(app_handle: &tauri::AppHandle) -> String {
    let bin_name: &str = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let sidecar: PathBuf = get_binary_dir(app_handle).join(bin_name);
    if sidecar.exists() {
        return sidecar.to_string_lossy().to_string();
    }
    if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }.to_string()
}

/// Fetch video metadata via yt-dlp --dump-json
pub async fn fetch_video_info(ytdlp: &str, url: &str) -> AppResult<VideoInfo> {
    let output = create_hidden_command(ytdlp)
        .args(["--dump-json", "--no-download", "--no-warnings", url])
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

    let mut args = vec![
        "--newline".to_string(),
        "--progress".to_string(),
        "--no-warnings".to_string(),
        "--ffmpeg-location".to_string(),
        ffmpeg.to_string(),
        "-o".to_string(),
        output_template.clone(),
        "--print".to_string(),
        "after_move:filepath".to_string(),
    ];

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

    let mut child = create_hidden_command(ytdlp)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Download(format!("Failed to spawn yt-dlp: {}", e)))?;

    let stdout = child.stdout.take().unwrap();
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
            // Check if this line is a file path (from --print after_move:filepath)
            let trimmed = line.trim();
            if !trimmed.starts_with('[') && !trimmed.contains('%') && !trimmed.is_empty() {
                if std::path::Path::new(trimmed).extension().is_some() {
                    let mut path = output_path_clone.lock().await;
                    *path = trimmed.to_string();
                }
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
