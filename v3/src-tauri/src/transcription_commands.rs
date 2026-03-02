use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Database;
use crate::download;

/// Helper macro for transcription error handling — avoids repeating the
/// "update DB + emit error event + return" pattern ~15 times.
macro_rules! transcription_bail {
    ($db:expr, $app:expr, $id:expr, $err:expr) => {{
        let err_msg = $err.to_string();
        if let Ok(db_lock) = $db.lock() {
            let _ = db_lock.update_transcript_error($id, &err_msg);
        }
        let _ = $app.emit(
            "transcription-progress",
            serde_json::json!({
                "id": $id,
                "progress": 0.0,
                "status": "error",
                "error": err_msg
            }),
        );
        return;
    }};
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
            #[cfg(target_os = "android")]
            {
                // On Android, use Termux to extract audio via yt-dlp
                let (installed, has_perm) = crate::android_bridge::termux_info();
                if !installed || !has_perm {
                    transcription_bail!(db_clone, app_clone, &id_clone,
                        "Audio extraction requires Termux with yt-dlp. Please complete Android setup first.");
                }

                let check_dir = crate::tool_install_commands::get_shared_check_dir();
                let transcribe_dir = format!("{}/Download/YTDL/.transcribe",
                    std::env::var("EXTERNAL_STORAGE").unwrap_or_else(|_| "/sdcard".to_string()));
                let audio_file = format!("{}/{}.mp3", transcribe_dir, id_clone);
                let status_file = format!("{}/transcribe_{}.txt", check_dir, id_clone);
                let _ = std::fs::remove_file(&status_file);

                let escaped_url = format!("'{}'", source_clone.replace('\'', "'\\''"));
                let command = format!(
                    "mkdir -p '{}' && yt-dlp -x --audio-format mp3 --audio-quality 0 --no-warnings --no-playlist -o '{}/{}.%(ext)s' {} 2>&1 ; echo \"TRANSCRIBE_EXIT:$?\"",
                    transcribe_dir, transcribe_dir, id_clone, escaped_url
                );

                log::info!("[start_transcription] Extracting audio via Termux for {}", id_clone);
                match crate::android_bridge::run_termux_check(&command, &status_file) {
                    Ok(true) => {}
                    Ok(false) => {
                        transcription_bail!(db_clone, app_clone, &id_clone,
                            "Failed to send audio extraction command to Termux");
                    }
                    Err(e) => {
                        transcription_bail!(db_clone, app_clone, &id_clone,
                            format!("Termux audio extraction error: {}", e));
                    }
                }

                // Poll for completion (up to 5 minutes)
                let audio_path_buf = PathBuf::from(&audio_file);
                let mut found = false;
                for i in 0..100 {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    if let Ok(content) = tokio::fs::read_to_string(&status_file).await {
                        let trimmed = content.trim();
                        if trimmed.contains("TRANSCRIBE_EXIT:0") {
                            found = true;
                            break;
                        } else if trimmed.contains("TRANSCRIBE_EXIT:") {
                            let _ = tokio::fs::remove_file(&status_file).await;
                            // Extract error message (lines before the exit marker)
                            let err_msg = trimmed.lines()
                                .filter(|l| !l.starts_with("TRANSCRIBE_EXIT:"))
                                .collect::<Vec<_>>()
                                .join("\n");
                            transcription_bail!(db_clone, app_clone, &id_clone,
                                format!("Audio extraction failed: {}", err_msg.chars().take(300).collect::<String>()));
                        }
                    }
                    if i > 0 && i % 10 == 0 {
                        log::info!("[start_transcription] Still waiting for audio extraction... {}s", i * 3);
                    }
                }

                let _ = tokio::fs::remove_file(&status_file).await;

                if !found {
                    transcription_bail!(db_clone, app_clone, &id_clone,
                        "Audio extraction via Termux timed out (5 minutes)");
                }

                if !audio_path_buf.exists() {
                    // Try to find any audio file with the ID prefix
                    let mut fallback_path = None;
                    if let Ok(mut entries) = tokio::fs::read_dir(&transcribe_dir).await {
                        while let Ok(Some(entry)) = entries.next_entry().await {
                            let fname = entry.file_name().to_string_lossy().to_string();
                            if fname.starts_with(&id_clone) {
                                fallback_path = Some(entry.path());
                                break;
                            }
                        }
                    }
                    match fallback_path {
                        Some(p) => {
                            temp_files.push(p.clone());
                            p
                        }
                        None => {
                            transcription_bail!(db_clone, app_clone, &id_clone,
                                "Audio extraction completed but output file not found");
                        }
                    }
                } else {
                    temp_files.push(audio_path_buf.clone());
                    audio_path_buf
                }
            }

            #[cfg(not(target_os = "android"))]
            {
                let temp_dir = match app_clone.path().temp_dir() {
                    Ok(dir) => dir,
                    Err(e) => {
                        transcription_bail!(db_clone, app_clone, &id_clone, e);
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
                            transcription_bail!(db_clone, app_clone, &id_clone, stderr.trim());
                        }
                    }
                    Err(e) => {
                        transcription_bail!(db_clone, app_clone, &id_clone, e);
                    }
                }

                if !output_audio.exists() {
                    transcription_bail!(db_clone, app_clone, &id_clone, "Audio download failed: output file not found");
                }

                temp_files.push(output_audio.clone());
                output_audio
            }
        } else {
            PathBuf::from(source_clone)
        };

        let (text, language) = if provider_clone == "local" {
            if whisper_cpp_clone.is_empty() || whisper_model_clone.is_empty() {
                transcription_bail!(db_clone, app_clone, &id_clone, "Local transcription requires whisper_cpp_path and whisper_model_path. Please run setup first.");
            }

            // ── Android: run whisper-cli via Termux ──
            #[cfg(target_os = "android")]
            {
                let external = std::env::var("EXTERNAL_STORAGE").unwrap_or_else(|_| "/sdcard".to_string());
                let transcribe_dir = format!("{}/Download/YTDL/.transcribe", external);
                let output_txt = format!("{}/{}.txt", transcribe_dir, id_clone);
                let audio_path_str = audio_path.to_string_lossy().to_string();

                let check_dir = crate::tool_install_commands::get_shared_check_dir();
                let status_file = format!("{}/whisper_run_{}.txt", check_dir, id_clone);
                let _ = std::fs::remove_file(&status_file);

                // whisper-cli -m model -f audio -otxt -of output_base
                let output_base = format!("{}/{}", transcribe_dir, id_clone);
                let whisper_cmd = format!(
                    "mkdir -p '{}' && '{}' -m '{}' -f '{}' -otxt -of '{}' 2>&1; echo \"WHISPER_EXIT:$?\"",
                    transcribe_dir, whisper_cpp_clone, whisper_model_clone,
                    audio_path_str, output_base
                );

                log::info!("[start_transcription] Running whisper via Termux for {}", id_clone);
                match crate::android_bridge::run_termux_check(&whisper_cmd, &status_file) {
                    Ok(true) => {}
                    Ok(false) => {
                        transcription_bail!(db_clone, app_clone, &id_clone,
                            "Failed to send whisper command to Termux");
                    }
                    Err(e) => {
                        transcription_bail!(db_clone, app_clone, &id_clone,
                            format!("Termux whisper error: {}", e));
                    }
                }

                // Poll for completion (up to 30 minutes for large files)
                let mut whisper_done = false;
                for i in 0..360 {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                    // Check for cancellation
                    if *cancel_rx_clone.borrow() {
                        let _ = tokio::fs::remove_file(&status_file).await;
                        return;
                    }

                    if let Ok(content) = tokio::fs::read_to_string(&status_file).await {
                        let trimmed = content.trim();
                        if trimmed.contains("WHISPER_EXIT:0") {
                            whisper_done = true;
                            break;
                        } else if trimmed.contains("WHISPER_EXIT:") {
                            let _ = tokio::fs::remove_file(&status_file).await;
                            let err_msg = trimmed.lines()
                                .filter(|l| !l.starts_with("WHISPER_EXIT:"))
                                .collect::<Vec<_>>()
                                .join("\n");
                            transcription_bail!(db_clone, app_clone, &id_clone,
                                format!("whisper.cpp failed: {}", err_msg.chars().take(500).collect::<String>()));
                        }
                    }
                    if i > 0 && i % 12 == 0 {
                        log::info!("[start_transcription] Whisper still running... {}min", i * 5 / 60);
                        let _ = app_clone.emit(
                            "transcription-progress",
                            serde_json::json!({
                                "id": id_clone,
                                "progress": 50.0,
                                "status": "processing"
                            }),
                        );
                    }
                }
                let _ = tokio::fs::remove_file(&status_file).await;

                if !whisper_done {
                    transcription_bail!(db_clone, app_clone, &id_clone,
                        "Whisper transcription via Termux timed out (30 minutes)");
                }

                // Read output text
                let text = match tokio::fs::read_to_string(&output_txt).await {
                    Ok(t) => {
                        // Clean up output file
                        let _ = tokio::fs::remove_file(&output_txt).await;
                        t
                    }
                    Err(e) => {
                        transcription_bail!(db_clone, app_clone, &id_clone,
                            format!("Failed to read whisper output: {}", e));
                    }
                };

                (text, String::new())
            }

            // ── Desktop: run whisper-cli directly ──
            #[cfg(not(target_os = "android"))]
            {
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
                        transcription_bail!(db_clone, app_clone, &id_clone, e);
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
                        transcription_bail!(db_clone, app_clone, &id_clone, format!("Failed to extract audio from media file: {}", stderr.trim()));
                    }
                    Err(e) => {
                        transcription_bail!(db_clone, app_clone, &id_clone, format!("Failed to run ffmpeg for local transcription: {}", e));
                    }
                }
            }

            let temp_dir = match app_clone.path().temp_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    transcription_bail!(db_clone, app_clone, &id_clone, e);
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
                    transcription_bail!(db_clone, app_clone, &id_clone, e);
                }
            };

            let status = tokio::select! {
                result = child.wait() => {
                    match result {
                        Ok(status) => Some(status),
                        Err(e) => {
                            transcription_bail!(db_clone, app_clone, &id_clone, e);
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
                transcription_bail!(db_clone, app_clone, &id_clone, format!("whisper.cpp exited with status {}", status));
            }

            let text = match tokio::fs::read_to_string(&output_txt).await {
                Ok(t) => t,
                Err(e) => {
                    transcription_bail!(db_clone, app_clone, &id_clone, e);
                }
            };

            (text, String::new())
            } // #[cfg(not(target_os = "android"))]
        } else {
            let api_key = if !api_key_clone.is_empty() {
                api_key_clone
            } else {
                transcription_bail!(db_clone, app_clone, &id_clone, "OpenAI API key is missing");
            };

            let bytes = match tokio::fs::read(&audio_path).await {
                Ok(b) => b,
                Err(e) => {
                    transcription_bail!(db_clone, app_clone, &id_clone, e);
                }
            };

            // OpenAI Whisper API has a 25 MB file-size limit
            const MAX_UPLOAD_SIZE: usize = 25 * 1024 * 1024;
            if bytes.len() > MAX_UPLOAD_SIZE {
                transcription_bail!(
                    db_clone, app_clone, &id_clone,
                    format!(
                        "Audio file is too large ({:.1} MB). OpenAI Whisper API limit is 25 MB. \
                         Try a shorter clip or use local transcription.",
                        bytes.len() as f64 / (1024.0 * 1024.0)
                    )
                );
            }

            let model = if !model_override_clone.is_empty() {
                model_override_clone
            } else {
                api_model_clone
            };

            let part = reqwest::multipart::Part::bytes(bytes).file_name("audio.mp3");
            let form = reqwest::multipart::Form::new()
                .text("model", model)
                .part("file", part);

            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    transcription_bail!(db_clone, app_clone, &id_clone, e);
                }
            };
            let response = match client
                .post("https://api.openai.com/v1/audio/transcriptions")
                .bearer_auth(api_key)
                .multipart(form)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    transcription_bail!(db_clone, app_clone, &id_clone, e);
                }
            };

            if !response.status().is_success() {
                let body = response.text().await.unwrap_or_default();
                transcription_bail!(db_clone, app_clone, &id_clone, body);
            }

            let json: serde_json::Value = match response.json().await {
                Ok(v) => v,
                Err(e) => {
                    transcription_bail!(db_clone, app_clone, &id_clone, e);
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
        .timeout(std::time::Duration::from_secs(15))
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

    #[cfg(target_os = "android")]
    {
        return install_local_transcription_android(app, db, &model_id, model_filename).await;
    }

    #[cfg(not(target_os = "android"))]
    {

    if !cfg!(target_os = "windows") {
        return Err("Automatic local transcription install is currently supported on Windows only. Please use the API (Cloud) option.".to_string());
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
        .timeout(std::time::Duration::from_secs(300))
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
            let Some(filename) = std::path::Path::new(&entry_name).file_name() else {
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

    } // #[cfg(not(target_os = "android"))]
}

/// Install whisper.cpp and model via Termux on Android.
#[cfg(target_os = "android")]
async fn install_local_transcription_android(
    app: AppHandle,
    db: State<'_, Arc<Mutex<Database>>>,
    model_id: &str,
    model_filename: &str,
) -> Result<serde_json::Value, String> {
    use tauri::Emitter;

    let (installed, has_perm) = crate::android_bridge::termux_info();
    if !installed || !has_perm {
        return Err("Local transcription requires Termux. Please complete Android setup first.".to_string());
    }

    let check_dir = crate::tool_install_commands::get_shared_check_dir();
    let external = std::env::var("EXTERNAL_STORAGE").unwrap_or_else(|_| "/sdcard".to_string());
    let model_dir = format!("{}/Download/YTDL/.whisper/models", external);

    // Step 1: Check if whisper-cli is already installed in Termux
    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "whisper.cpp",
        "status": "checking",
        "progress": 5
    }));

    let check_file = format!("{}/whisper_check_{}.txt", check_dir, uuid::Uuid::new_v4());
    let _ = std::fs::remove_file(&check_file);

    let check_cmd = "which whisper-cli 2>/dev/null || which whisper-cpp 2>/dev/null || echo 'NOT_FOUND'";
    match crate::android_bridge::run_termux_check(check_cmd, &check_file) {
        Ok(true) => {}
        _ => return Err("Failed to communicate with Termux".to_string()),
    }

    let mut whisper_path = String::new();
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if let Ok(content) = tokio::fs::read_to_string(&check_file).await {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                if trimmed.contains("NOT_FOUND") {
                    break; // Need to install
                } else {
                    whisper_path = trimmed.lines().next().unwrap_or("").trim().to_string();
                    break;
                }
            }
        }
    }
    let _ = tokio::fs::remove_file(&check_file).await;

    // Step 2: Install whisper.cpp via pkg if not found
    if whisper_path.is_empty() {
        let _ = app.emit("install-progress", serde_json::json!({
            "tool": "whisper.cpp",
            "status": "downloading",
            "progress": 15
        }));

        let install_file = format!("{}/whisper_install_{}.txt", check_dir, uuid::Uuid::new_v4());
        let _ = std::fs::remove_file(&install_file);

        // Install repos first, then try package install.
        // Some Termux mirrors don't provide whisper-cpp; in that case build from source.
        let install_cmd = concat!(
            "pkg update -y 2>&1; ",
            "if ! command -v whisper-cli >/dev/null 2>&1 && ! command -v whisper-cpp >/dev/null 2>&1; then ",
            "  pkg install -y tur-repo 2>&1; ",
            "  pkg update -y 2>&1; ",
            "  pkg install -y whisper-cpp 2>&1 || pkg install -y whisper.cpp 2>&1 || true; ",
            "fi; ",
            "if ! command -v whisper-cli >/dev/null 2>&1 && ! command -v whisper-cpp >/dev/null 2>&1; then ",
            "  pkg install -y git cmake make clang pkg-config binutils 2>&1; ",
            "  mkdir -p $HOME/.local/src 2>/dev/null; ",
            "  if [ ! -d $HOME/.local/src/whisper.cpp ]; then git clone --depth=1 https://github.com/ggml-org/whisper.cpp $HOME/.local/src/whisper.cpp 2>&1; fi; ",
            "  cd $HOME/.local/src/whisper.cpp && cmake -B build -DWHISPER_SDL2=OFF 2>&1 && cmake --build build -j2 2>&1; ",
            "  if [ -x $HOME/.local/src/whisper.cpp/build/bin/whisper-cli ]; then mkdir -p $PREFIX/bin 2>/dev/null; ln -sf $HOME/.local/src/whisper.cpp/build/bin/whisper-cli $PREFIX/bin/whisper-cli 2>&1; fi; ",
            "  if [ ! -x $PREFIX/bin/whisper-cli ] && [ -x $HOME/.local/src/whisper.cpp/build/bin/main ]; then ln -sf $HOME/.local/src/whisper.cpp/build/bin/main $PREFIX/bin/whisper-cli 2>&1; fi; ",
            "fi; ",
            "which whisper-cli 2>/dev/null || which whisper-cpp 2>/dev/null || echo 'INSTALL_FAILED'"
        );

        match crate::android_bridge::run_termux_check(install_cmd, &install_file) {
            Ok(true) => {}
            _ => return Err("Failed to send install command to Termux".to_string()),
        }

        // Poll for result (up to 20 minutes; source build on Android can be slow)
        for i in 0..800 {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            if let Ok(content) = tokio::fs::read_to_string(&install_file).await {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    if trimmed.contains("INSTALL_FAILED") {
                        let _ = tokio::fs::remove_file(&install_file).await;
                        return Err(
                            "Failed to install whisper.cpp in Termux. \
                             Try manually: pkg update -y && pkg install -y tur-repo && pkg update -y && (pkg install -y whisper-cpp || pkg install -y whisper.cpp). \
                             If package is unavailable on your mirror, build from source: pkg install -y git cmake make clang pkg-config && git clone --depth=1 https://github.com/ggml-org/whisper.cpp $HOME/.local/src/whisper.cpp && cd $HOME/.local/src/whisper.cpp && cmake -B build -DWHISPER_SDL2=OFF && cmake --build build -j2 && ln -sf $HOME/.local/src/whisper.cpp/build/bin/whisper-cli $PREFIX/bin/whisper-cli"
                                .to_string(),
                        );
                    }
                    // Last line should be the path
                    whisper_path = trimmed.lines().last().unwrap_or("").trim().to_string();
                    if !whisper_path.is_empty() && !whisper_path.contains("INSTALL_FAILED") {
                        break;
                    }
                }
            }
            if i > 0 && i % 40 == 0 {
                log::info!("[install_local_transcription] Still installing whisper-cpp... {}s", i * 3 / 2);
            }
        }
        let _ = tokio::fs::remove_file(&install_file).await;

        if whisper_path.is_empty() || whisper_path.contains("INSTALL_FAILED") {
            return Err(
                "Timed out installing whisper.cpp (source build can be slow on Android). \
                 Try manually: pkg update -y && pkg install -y tur-repo && pkg update -y && (pkg install -y whisper-cpp || pkg install -y whisper.cpp). \
                 If unavailable, build from source (ggml-org/whisper.cpp) and ensure 'whisper-cli' is in PATH (or symlink build/bin/main to $PREFIX/bin/whisper-cli)."
                    .to_string(),
            );
        }
    }

    // Step 3: Download model file if not present
    let model_path = format!("{}/{}", model_dir, model_filename);

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "whisper-model",
        "status": "downloading",
        "progress": 50
    }));

    let model_check_file = format!("{}/whisper_model_{}.txt", check_dir, uuid::Uuid::new_v4());
    let _ = std::fs::remove_file(&model_check_file);

    let model_url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}?download=true",
        model_filename
    );

    // Check if model already exists, if not download via curl in Termux
    let model_cmd = format!(
        "mkdir -p '{}' && if [ -f '{}' ]; then echo 'MODEL_EXISTS'; else curl -L -o '{}' '{}' 2>&1 && echo 'MODEL_DOWNLOADED' || echo 'MODEL_DOWNLOAD_FAILED'; fi",
        model_dir, model_path, model_path, model_url
    );

    match crate::android_bridge::run_termux_check(&model_cmd, &model_check_file) {
        Ok(true) => {}
        _ => return Err("Failed to send model download command to Termux".to_string()),
    }

    // Poll for model download (up to 10 minutes for large models)
    let mut model_ok = false;
    for i in 0..200 {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        if let Ok(content) = tokio::fs::read_to_string(&model_check_file).await {
            let trimmed = content.trim();
            if trimmed.contains("MODEL_EXISTS") || trimmed.contains("MODEL_DOWNLOADED") {
                model_ok = true;
                break;
            }
            if trimmed.contains("MODEL_DOWNLOAD_FAILED") {
                let _ = tokio::fs::remove_file(&model_check_file).await;
                return Err(format!("Failed to download whisper model '{}' in Termux", model_filename));
            }
        }
        if i > 0 && i % 20 == 0 {
            log::info!("[install_local_transcription] Still downloading model... {}s", i * 3);
        }
    }
    let _ = tokio::fs::remove_file(&model_check_file).await;

    if !model_ok {
        return Err("Timed out downloading whisper model. Please try again.".to_string());
    }

    // Step 4: Save settings
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock.save_setting("whisper_cpp_path", &whisper_path).map_err(|e| e.to_string())?;
        db_lock.save_setting("whisper_model_path", &model_path).map_err(|e| e.to_string())?;
        db_lock.save_setting("local_model_id", model_id).map_err(|e| e.to_string())?;
        db_lock.save_setting("transcribe_provider", "local").map_err(|e| e.to_string())?;
        db_lock.save_setting("transcription_configured", "true").map_err(|e| e.to_string())?;
    }

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "whisper-local",
        "status": "completed",
        "progress": 100
    }));

    Ok(serde_json::json!({
        "ok": true,
        "modelId": model_id,
        "whisperCppPath": whisper_path,
        "whisperModelPath": model_path,
    }))
}
