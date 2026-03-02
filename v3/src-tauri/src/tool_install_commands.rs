use tauri::{AppHandle, Emitter};

use crate::download;

fn ensure_tool_bin_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let bin_dir = download::get_binary_dir(app);
    
    // Try to create directory with detailed error info
    match std::fs::create_dir_all(&bin_dir) {
        Ok(()) => {
            // Verify we can write to the directory
            let test_file = bin_dir.join(".write_test");
            match std::fs::write(&test_file, b"test") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&test_file);
                }
                Err(e) => {
                    return Err(format!(
                        "Cannot write to binary directory '{}': {}. Please check directory permissions.",
                        bin_dir.display(), e
                    ));
                }
            }
            Ok(bin_dir)
        }
        Err(e) => {
            let error_msg = if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!(
                    "Permission denied when creating binary directory '{}'. Please check app permissions or reinstall the application.",
                    bin_dir.display()
                )
            } else {
                format!("Failed to create binary directory '{}': {}", bin_dir.display(), e)
            };
            Err(error_msg)
        }
    }
}

/// Get a shared directory for Termux check output files.
/// Uses shared storage `.checks/` dir accessible by both our app and Termux.
#[cfg(target_os = "android")]
pub(crate) fn get_shared_check_dir() -> String {
    download::android_shared_checks_dir()
}

/// Try executing a program with --version and return Ok(true) if it succeeds.
#[cfg(target_os = "android")]
async fn try_exec_version(program: &str, version_flag: &str) -> bool {
    match download::create_hidden_command(program)
        .arg(version_flag)
        .output()
        .await
    {
        Ok(out) if out.status.success() => {
            log::info!("[try_exec_version] '{}' works", program);
            true
        }
        Ok(out) => {
            log::debug!("[try_exec_version] '{}' ran but failed: {:?}", program,
                String::from_utf8_lossy(&out.stderr).trim());
            false
        }
        Err(e) => {
            log::debug!("[try_exec_version] '{}' failed to exec: {} (os_err={:?})", program, e, e.raw_os_error());
            false
        }
    }
}

#[tauri::command]
pub async fn check_ytdlp(app: AppHandle) -> Result<bool, String> {
    let ytdlp = download::get_ytdlp_path(&app);
    log::info!("[check_ytdlp] Resolved path: {}", ytdlp);

    // On Android: try bundled binary first, then Termux via RUN_COMMAND Intent
    #[cfg(target_os = "android")]
    {
        if let Some(native_dir) = download::get_native_lib_dir() {
            log::info!("[check_ytdlp] nativeLibraryDir: {}", native_dir.display());
            if let Ok(entries) = std::fs::read_dir(&native_dir) {
                for entry in entries.flatten() {
                    log::info!("[check_ytdlp] nativeLib: {}", entry.path().display());
                }
            }
        } else {
            log::warn!("[check_ytdlp] nativeLibraryDir not found — jniLibs may not be bundled");
        }

        // Attempt 1: resolved path (libytdlp.so bundled binary)
        if try_exec_version(&ytdlp, "--version").await {
            return Ok(true);
        }

        // Attempt 2: Termux RUN_COMMAND via JNI bridge
        // This is the primary method on Android since SELinux blocks direct
        // cross-app binary execution.
        let (termux_installed, termux_has_perm) = crate::android_bridge::termux_info();
        if termux_installed && termux_has_perm {
            log::info!("[check_ytdlp] Trying Termux RUN_COMMAND bridge...");
            let check_dir = get_shared_check_dir();
            let output_file = format!("{}/check_ytdlp.txt", check_dir);
            let _ = std::fs::remove_file(&output_file);

            match crate::android_bridge::run_termux_check(
                "yt-dlp --version",
                &output_file,
            ) {
                Ok(true) => {
                    log::info!("[check_ytdlp] Intent sent, polling for result at {}", output_file);
                    // Poll for result (Termux runs async via Intent)
                    // Increased to 30 iterations × 500ms = 15s to handle Termux cold-start
                    for i in 0..30 {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        match std::fs::read_to_string(&output_file) {
                            Ok(content) => {
                                let trimmed = content.trim();
                                if !trimmed.is_empty() {
                                    let _ = std::fs::remove_file(&output_file);
                                    // Check if it looks like a version string (e.g., "2025.01.15")
                                    if trimmed.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                        log::info!("[check_ytdlp] Termux yt-dlp version: {}", trimmed);
                                        return Ok(true);
                                    } else {
                                        log::warn!("[check_ytdlp] Termux yt-dlp check returned: {}", trimmed);
                                        return Ok(false);
                                    }
                                }
                            }
                            Err(e) if i == 0 => {
                                log::debug!("[check_ytdlp] Waiting for output file ({}): {}", output_file, e);
                            }
                            Err(_) => {}
                        }
                    }
                    log::warn!("[check_ytdlp] Termux check timed out after 15 seconds — \
                               check that Termux has storage permission (termux-setup-storage) \
                               and that this app has MANAGE_EXTERNAL_STORAGE");
                }
                Ok(false) => {
                    log::warn!("[check_ytdlp] Termux RUN_COMMAND intent failed to send");
                }
                Err(e) => {
                    log::warn!("[check_ytdlp] Termux JNI bridge error: {}", e);
                }
            }
        } else {
            log::info!(
                "[check_ytdlp] Termux not available (installed={}, perm={}). \
                 Bundled Linux binaries cannot run on Android due to ELF interpreter mismatch. \
                 Install Termux from F-Droid and enable allow-external-apps.",
                termux_installed, termux_has_perm
            );
        }

        return Ok(false);
    }

    // Desktop / non-Android path
    #[cfg(not(target_os = "android"))]
    {
        if !std::path::Path::new(&ytdlp).exists() {
            log::warn!("[check_ytdlp] yt-dlp not found at: {}", ytdlp);
            return Ok(false);
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&ytdlp) {
                let mode = metadata.permissions().mode();
                log::info!("[check_ytdlp] permissions: {:o}", mode);
                if mode & 0o111 == 0 {
                    log::error!("[check_ytdlp] yt-dlp at {} is NOT executable! Permissions: {:o}", ytdlp, mode);
                }
            }
        }

        let result = download::create_hidden_command(&ytdlp)
            .arg("--version")
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                log::info!("[check_ytdlp] version: {}", stdout.trim());
                Ok(true)
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::warn!("[check_ytdlp] exists but failed to run: {}", stderr.trim());
                Ok(false)
            }
            Err(e) => {
                let code = e.raw_os_error().unwrap_or(0);
                if code == 13 {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if std::fs::set_permissions(&ytdlp, std::fs::Permissions::from_mode(0o755)).is_ok() {
                            if let Ok(out) = download::create_hidden_command(&ytdlp).arg("--version").output().await {
                                if out.status.success() {
                                    log::info!("[check_ytdlp] works after chmod!");
                                    return Ok(true);
                                }
                            }
                        }
                    }
                    return Err(format!(
                        "Permission denied (OS error 13). Try: chmod +x {}",
                        ytdlp
                    ));
                }
                log::error!("[check_ytdlp] failed to run '{}': {} (err={})", ytdlp, e, code);
                Ok(false)
            }
        }
    }
}

#[tauri::command]
pub async fn check_ffmpeg(app: AppHandle) -> Result<bool, String> {
    let ffmpeg = download::get_ffmpeg_path(&app);
    log::info!("[check_ffmpeg] Resolved path: {}", ffmpeg);

    // On Android: try bundled binary first, then Termux via RUN_COMMAND Intent
    #[cfg(target_os = "android")]
    {
        // Attempt 1: resolved path (libffmpeg.so bundled binary)
        if try_exec_version(&ffmpeg, "-version").await {
            return Ok(true);
        }

        // Attempt 2: Termux RUN_COMMAND via JNI bridge
        let (termux_installed, termux_has_perm) = crate::android_bridge::termux_info();
        if termux_installed && termux_has_perm {
            log::info!("[check_ffmpeg] Trying Termux RUN_COMMAND bridge...");
            let check_dir = get_shared_check_dir();
            let output_file = format!("{}/check_ffmpeg.txt", check_dir);
            let _ = std::fs::remove_file(&output_file);

            match crate::android_bridge::run_termux_check(
                "ffmpeg -version 2>&1 | head -1",
                &output_file,
            ) {
                Ok(true) => {
                    log::info!("[check_ffmpeg] Intent sent, polling for result at {}", output_file);
                    for i in 0..30 {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        match std::fs::read_to_string(&output_file) {
                            Ok(content) => {
                                let trimmed = content.trim();
                                if !trimmed.is_empty() {
                                    let _ = std::fs::remove_file(&output_file);
                                    if trimmed.contains("ffmpeg") {
                                        log::info!("[check_ffmpeg] Termux ffmpeg: {}", trimmed);
                                        return Ok(true);
                                    } else {
                                        log::warn!("[check_ffmpeg] Termux ffmpeg check returned: {}", trimmed);
                                        return Ok(false);
                                    }
                                }
                            }
                            Err(e) if i == 0 => {
                                log::debug!("[check_ffmpeg] Waiting for output file: {}", e);
                            }
                            Err(_) => {}
                        }
                    }
                    log::warn!("[check_ffmpeg] Termux check timed out after 15 seconds");
                }
                Ok(false) => {
                    log::warn!("[check_ffmpeg] Termux RUN_COMMAND intent failed to send");
                }
                Err(e) => {
                    log::warn!("[check_ffmpeg] Termux JNI bridge error: {}", e);
                }
            }
        } else {
            log::info!(
                "[check_ffmpeg] Termux not available (installed={}, perm={}). \
                 Install Termux from F-Droid and enable allow-external-apps.",
                termux_installed, termux_has_perm
            );
        }

        return Ok(false);
    }

    // Desktop / non-Android path
    #[cfg(not(target_os = "android"))]
    {
        if !std::path::Path::new(&ffmpeg).exists() {
            log::debug!("[check_ffmpeg] not found at: {}", ffmpeg);
            return Ok(false);
        }

        let result = download::create_hidden_command(&ffmpeg)
            .arg("-version")
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                log::debug!("[check_ffmpeg] found at: {}", ffmpeg);
                Ok(true)
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::warn!("[check_ffmpeg] exists but failed: {}", stderr.trim());
                Ok(false)
            }
            Err(e) => {
                if e.raw_os_error() == Some(13) {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(&ffmpeg, std::fs::Permissions::from_mode(0o755));
                        if let Ok(out) = download::create_hidden_command(&ffmpeg).arg("-version").output().await {
                            if out.status.success() {
                                log::info!("[check_ffmpeg] works after chmod!");
                                return Ok(true);
                            }
                        }
                    }
                    return Err(format!("Permission denied. Try: chmod +x {}", ffmpeg));
                }
                log::error!("[check_ffmpeg] failed to run '{}': {}", ffmpeg, e);
                Ok(false)
            }
        }
    }
}

/// Install yt-dlp binary from GitHub releases.
#[tauri::command]
pub async fn install_ytdlp(app: AppHandle) -> Result<(), String> {
    // On Android, check if yt-dlp is already bundled in nativeLibraryDir
    #[cfg(target_os = "android")]
    {
        log::info!("[install_ytdlp] Android: checking bundled binaries first");
        
        // Check if bundled in nativeLibraryDir (from jniLibs during APK build)
        if let Some(native_dir) = download::get_native_lib_dir() {
            let bundled_ytdlp = native_dir.join("libytdlp.so");
            if bundled_ytdlp.exists() {
                log::info!("[install_ytdlp] Found bundled yt-dlp at: {}", bundled_ytdlp.display());
                
                // Test if the bundled binary works
                let test = download::create_hidden_command(&bundled_ytdlp.to_string_lossy())
                    .arg("--version")
                    .output()
                    .await;
                
                match test {
                    Ok(output) if output.status.success() => {
                        let version = String::from_utf8_lossy(&output.stdout);
                        log::info!("[install_ytdlp] Bundled yt-dlp works! Version: {}", version.trim());
                        
                        let _ = app.emit("install-progress", serde_json::json!({
                            "tool": "yt-dlp",
                            "status": "completed",
                            "progress": 100
                        }));
                        return Ok(());
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        log::warn!("[install_ytdlp] Bundled yt-dlp binary failed: {}", stderr.trim());
                    }
                    Err(e) => {
                        log::warn!("[install_ytdlp] Cannot execute bundled yt-dlp: {}. Trying alternatives...", e);
                    }
                }
            }
        }
        
        // Check if Termux has yt-dlp installed
        let termux_paths = [
            "/data/data/com.termux/files/usr/bin/yt-dlp",
            "/data/data/com.termux/files/usr/local/bin/yt-dlp",
        ];
        for termux_path in &termux_paths {
            if std::path::Path::new(termux_path).exists() {
                let test = tokio::process::Command::new(termux_path)
                    .arg("--version")
                    .output()
                    .await;
                if let Ok(output) = test {
                    if output.status.success() {
                        log::info!("[install_ytdlp] Found working yt-dlp in Termux: {}", termux_path);
                        let _ = app.emit("install-progress", serde_json::json!({
                            "tool": "yt-dlp",
                            "status": "completed",
                            "progress": 100
                        }));
                        return Ok(());
                    }
                }
            }
        }
        
        // Try Python/pip approach
        let bin_dir = ensure_tool_bin_dir(&app)?;
        match install_ytdlp_via_pip(&app, &bin_dir).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::warn!("[install_ytdlp] pip installation failed: {}", e);
            }
        }
        
        // Do not attempt download fallback on Android: app_data_dir is noexec on modern Android,
        // so downloaded executables will fail with OS error 13 and confuse users.
        return Err(
            "Android limitation: cannot install yt-dlp by downloading an executable inside the app.\n\
            Use one of these options:\n\
            1) Install a build where yt-dlp is bundled and executable from nativeLibraryDir\n\
            2) Install Termux and run: pkg install python && pip install yt-dlp"
                .to_string(),
        );
    }
    
    #[cfg(not(target_os = "android"))]
    {
    let bin_dir = ensure_tool_bin_dir(&app)?;

    let (url, filename) = if cfg!(target_os = "windows") {
        ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe", "yt-dlp.exe")
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

    let dl_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let response = dl_client.get(url).send().await.map_err(|e| format!("Download failed: {}. Please check your internet connection.", e))?;
    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}. Please try again later.", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| format!("Failed to read download: {}", e))?;

    // Verify SHA256 checksum against the official SHA2-256SUMS file
    let binary_basename = url.rsplit('/').next().unwrap_or(filename);
    let checksums_url = url.rsplit_once('/').map(|(base, _)| format!("{}/SHA2-256SUMS", base))
        .unwrap_or_else(|| "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS".to_string());

    match dl_client.get(&checksums_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(sums_text) = resp.text().await {
                use sha2::Digest;
                let computed = format!("{:x}", sha2::Sha256::digest(&bytes));
                let expected = sums_text.lines()
                    .find(|line| line.ends_with(binary_basename))
                    .and_then(|line| line.split_whitespace().next())
                    .map(|h| h.to_lowercase());

                if let Some(expected_hash) = expected {
                    if computed != expected_hash {
                        return Err(format!(
                            "SHA-256 checksum mismatch for {}!\nExpected: {}\nGot:      {}\nThe download may be corrupted or tampered with.",
                            binary_basename, expected_hash, computed
                        ));
                    }
                    log::info!("SHA-256 checksum verified for {}", binary_basename);
                } else {
                    log::warn!("Could not find checksum for '{}' in SHA2-256SUMS — skipping verification", binary_basename);
                }
            }
        }
        _ => {
            log::warn!("Could not fetch SHA2-256SUMS for checksum verification — skipping");
        }
    }

    let dest = bin_dir.join(filename);
    std::fs::write(&dest, &bytes).map_err(|e| format!("Failed to save {}: {}. Check if the directory is writable.", dest.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set executable permissions: {}. Please run 'chmod +x {}' manually.", e, dest.display()))?;
    }

    #[cfg(windows)]
    let _ = {};

    // Test if binary works
    let test_result = download::create_hidden_command(&dest.to_string_lossy())
        .arg("--version")
        .output()
        .await;

    match test_result {
        Ok(output) if output.status.success() => {
            log::info!("yt-dlp binary works!");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::warn!("yt-dlp binary may not work correctly: {}", stderr.trim());
        }
        Err(e) => {
            log::error!("yt-dlp binary test failed: {}", e);
        }
    }

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "yt-dlp",
        "status": "completed",
        "progress": 100
    }));

    log::info!("yt-dlp installed successfully to {}", dest.display());
    Ok(())
    }

}

/// Install yt-dlp via Python/pip and create a wrapper script
#[cfg(target_os = "android")]
async fn install_ytdlp_via_pip(_app: &AppHandle, bin_dir: &std::path::Path) -> Result<(), String> {
    log::info!("[install_ytdlp_via_pip] Starting pip installation for Android");
    
    // Check if python3 is available
    let python_check = tokio::process::Command::new("python3")
        .arg("--version")
        .output()
        .await;

    let python_cmd = match python_check {
        Ok(output) if output.status.success() => "python3",
        _ => {
            // Try python
            let python_check2 = tokio::process::Command::new("python")
                .arg("--version")
                .output()
                .await;
            match python_check2 {
                Ok(output) if output.status.success() => "python",
                _ => {
                    return Err("Python not found on Android. Please install Python via an app like Termux or Pydroid, or use the Termux app to install yt-dlp with: pkg install python && pip install yt-dlp".to_string());
                }
            }
        }
    };

    log::info!("[install_ytdlp_via_pip] Found Python: {}", python_cmd);

    // Check if pip is available
    let pip_check = tokio::process::Command::new(python_cmd)
        .args(["-m", "pip", "--version"])
        .output()
        .await;

    let pip_available = pip_check.map(|o| o.status.success()).unwrap_or(false);
    
    if !pip_available {
        return Err("pip not found. Please install pip or use Termux to install yt-dlp: pkg install python && pip install yt-dlp".to_string());
    }

    log::info!("[install_ytdlp_via_pip] pip is available");

    // Try to install yt-dlp via pip
    let install_result = tokio::process::Command::new(python_cmd)
        .args(["-m", "pip", "install", "--user", "yt-dlp"])
        .output()
        .await;

    match install_result {
        Ok(output) => {
            if output.status.success() {
                log::info!("[install_ytdlp_via_pip] yt-dlp installed via pip");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::warn!("[install_ytdlp_via_pip] pip install failed: {}", stderr.trim());
                return Err(format!("Failed to install yt-dlp via pip: {}", stderr.trim()));
            }
        }
        Err(e) => {
            return Err(format!("Failed to run pip: {}", e));
        }
    }

    // Find yt-dlp location
    let which_result = tokio::process::Command::new(python_cmd)
        .args(["-m", "pip", "show", "yt-dlp"])
        .output()
        .await;

    let _yt_dlp_path = match which_result {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse location from pip show output
            stdout.lines()
                .find(|l| l.starts_with("Location:"))
                .map(|l| l.trim_start_matches("Location: ").to_string())
                .map(|loc| format!("{}/yt_dlp/__main__.py", loc))
        }
        _ => None
    };

    // Create wrapper script that calls Python
    let wrapper_content = format!(
        "#!/system/bin/sh\n\
        # yt-dlp wrapper for Android\n\
        # This wrapper calls yt-dlp Python module\n\
        \n\
        exec {} -m yt_dlp \"$@\"\n",
        python_cmd
    );

    let wrapper_path = bin_dir.join("yt-dlp");
    std::fs::write(&wrapper_path, &wrapper_content)
        .map_err(|e| format!("Failed to write wrapper: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set wrapper permissions: {}", e))?;
    }

    log::info!("[install_ytdlp_via_pip] Created wrapper at {}", wrapper_path.display());

    // Test the wrapper
    let test_result = tokio::process::Command::new(&wrapper_path)
        .arg("--version")
        .output()
        .await;

    match test_result {
        Ok(output) if output.status.success() => {
            log::info!("[install_ytdlp_via_pip] Wrapper works!");
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("yt-dlp wrapper test failed: {}", stderr.trim()))
        }
        Err(e) => {
            Err(format!("Failed to test yt-dlp wrapper: {}", e))
        }
    }
}

/// Install ffmpeg binary.
#[tauri::command]
pub async fn install_ffmpeg(app: AppHandle) -> Result<(), String> {
    // On Android, check if ffmpeg is already bundled in nativeLibraryDir
    #[cfg(target_os = "android")]
    {
        log::info!("[install_ffmpeg] Android: checking bundled binaries first");
        
        if let Some(native_dir) = download::get_native_lib_dir() {
            let bundled_ffmpeg = native_dir.join("libffmpeg.so");
            if bundled_ffmpeg.exists() {
                log::info!("[install_ffmpeg] Found bundled ffmpeg at: {}", bundled_ffmpeg.display());
                
                let test = download::create_hidden_command(&bundled_ffmpeg.to_string_lossy())
                    .arg("-version")
                    .output()
                    .await;
                
                match test {
                    Ok(output) if output.status.success() => {
                        log::info!("[install_ffmpeg] Bundled ffmpeg works!");
                        let _ = app.emit("install-progress", serde_json::json!({
                            "tool": "ffmpeg",
                            "status": "completed",
                            "progress": 100
                        }));
                        return Ok(());
                    }
                    _ => {
                        log::warn!("[install_ffmpeg] Bundled ffmpeg exists but cannot execute");
                    }
                }
            }
        }
        
        // Check Termux
        let termux_ffmpeg = "/data/data/com.termux/files/usr/bin/ffmpeg";
        if std::path::Path::new(termux_ffmpeg).exists() {
            let test = tokio::process::Command::new(termux_ffmpeg)
                .arg("-version")
                .output()
                .await;
            if let Ok(output) = test {
                if output.status.success() {
                    log::info!("[install_ffmpeg] Found working ffmpeg in Termux");
                    let _ = app.emit("install-progress", serde_json::json!({
                        "tool": "ffmpeg",
                        "status": "completed",
                        "progress": 100
                    }));
                    return Ok(());
                }
            }
        }

        // Do not attempt download fallback on Android: app_data_dir is noexec on modern Android,
        // downloaded ffmpeg will fail with OS error 13.
        return Err(
            "Android limitation: cannot install ffmpeg by downloading an executable inside the app.\n\
            Use one of these options:\n\
            1) Install a build where ffmpeg is bundled and executable from nativeLibraryDir\n\
            2) Install Termux and run: pkg install ffmpeg"
                .to_string(),
        );
    }
    
    #[cfg(not(target_os = "android"))]
    {
    let bin_dir = ensure_tool_bin_dir(&app)?;

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "ffmpeg",
        "status": "downloading",
        "progress": 0
    }));

    let dl_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    if cfg!(target_os = "windows") {
        use std::io::{Read, Write};

        // Download ffmpeg ZIP
        let url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
        let response = dl_client.get(url).send().await.map_err(|e| format!("Download failed: {}. Please check your internet connection.", e))?;
        if !response.status().is_success() {
            return Err(format!("Download failed with status: {}. Please try again later.", response.status()));
        }
        let bytes = response.bytes().await.map_err(|e| format!("Failed to read download: {}", e))?;

        let _ = app.emit("install-progress", serde_json::json!({
            "tool": "ffmpeg",
            "status": "extracting",
            "progress": 50
        }));

        // Write to temp zip file
        let temp_zip = bin_dir.join("ffmpeg_temp.zip");
        std::fs::write(&temp_zip, &bytes).map_err(|e| format!("Failed to save ZIP file: {}. Check directory permissions.", e))?;

        // Extract ffmpeg.exe from ZIP
        let file = std::fs::File::open(&temp_zip).map_err(|e| format!("Failed to open ZIP file: {}", e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Failed to parse ZIP: {}. The file may be corrupted.", e))?;

        let mut found = false;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| format!("Failed to read ZIP entry: {}", e))?;
            let name = entry.name().to_lowercase();
            
            // Find ffmpeg.exe in the archive (it's in a subdirectory)
            if name.ends_with("bin/ffmpeg.exe") || name.ends_with("bin\\ffmpeg.exe") {
                let dest = bin_dir.join("ffmpeg.exe");
                let mut outfile = std::fs::File::create(&dest).map_err(|e| format!("Failed to create ffmpeg.exe: {}. Check directory permissions.", e))?;
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer).map_err(|e| format!("Failed to read from ZIP: {}", e))?;
                outfile.write_all(&buffer).map_err(|e| format!("Failed to write ffmpeg.exe: {}", e))?;
                found = true;
            }
            // Also extract ffprobe.exe if present
            if name.ends_with("bin/ffprobe.exe") || name.ends_with("bin\\ffprobe.exe") {
                let dest = bin_dir.join("ffprobe.exe");
                let mut outfile = std::fs::File::create(&dest).map_err(|e| format!("Failed to create ffprobe.exe: {}. Check directory permissions.", e))?;
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer).map_err(|e| format!("Failed to read from ZIP: {}", e))?;
                outfile.write_all(&buffer).map_err(|e| format!("Failed to write ffprobe.exe: {}", e))?;
            }
        }

        // Clean up temp zip
        let _ = std::fs::remove_file(&temp_zip);

        if !found {
            return Err("Could not find ffmpeg.exe in ZIP archive. The archive structure may have changed.".to_string());
        }

        // Post-download integrity check: verify extracted binary is valid
        let ffmpeg_exe = bin_dir.join("ffmpeg.exe");
        let meta = std::fs::metadata(&ffmpeg_exe)
            .map_err(|e| format!("ffmpeg.exe not found after extraction: {}", e))?;
        if meta.len() < 1_000_000 {
            let _ = std::fs::remove_file(&ffmpeg_exe);
            return Err("ffmpeg.exe is suspiciously small (<1 MB) — download may be corrupted. Please try again.".to_string());
        }
        let verify = tokio::process::Command::new(&ffmpeg_exe)
            .arg("-version")
            .output()
            .await;
        match verify {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.to_lowercase().contains("ffmpeg") {
                    let _ = std::fs::remove_file(&ffmpeg_exe);
                    return Err("Downloaded binary does not appear to be ffmpeg. Please try again.".to_string());
                }
                log::info!("ffmpeg integrity verified: {}", stdout.lines().next().unwrap_or("ok"));
            }
            _ => {
                let _ = std::fs::remove_file(&ffmpeg_exe);
                return Err("Downloaded ffmpeg.exe failed execution test — the file may be corrupted. Please try again.".to_string());
            }
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

        let ffmpeg_bytes = dl_client.get(ffmpeg_url).send()
            .await
            .map_err(|e| format!("Download failed: {}. Please check your internet connection.", e))?
            .bytes()
            .await
            .map_err(|e| format!("Failed to read download: {}", e))?;
        std::fs::write(&ffmpeg_dest, &ffmpeg_bytes).map_err(|e| format!("Failed to save ffmpeg: {}. Check directory permissions.", e))?;

        let _ = app.emit("install-progress", serde_json::json!({
            "tool": "ffmpeg",
            "status": "downloading",
            "progress": 75
        }));

        let ffprobe_bytes = dl_client.get(ffprobe_url).send()
            .await
            .map_err(|e| format!("Download failed: {}. Please check your internet connection.", e))?
            .bytes()
            .await
            .map_err(|e| format!("Failed to read download: {}", e))?;
        std::fs::write(&ffprobe_dest, &ffprobe_bytes).map_err(|e| format!("Failed to save ffprobe: {}. Check directory permissions.", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ffmpeg_dest, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("Failed to set executable permissions for ffmpeg: {}. Please run 'chmod +x {}' manually.", e, ffmpeg_dest.display()))?;
            std::fs::set_permissions(&ffprobe_dest, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("Failed to set executable permissions for ffprobe: {}. Please run 'chmod +x {}' manually.", e, ffprobe_dest.display()))?;
        }

        // Post-download integrity check: verify downloaded binary is valid
        let meta = std::fs::metadata(&ffmpeg_dest)
            .map_err(|e| format!("ffmpeg not found after download: {}", e))?;
        if meta.len() < 1_000_000 {
            let _ = std::fs::remove_file(&ffmpeg_dest);
            return Err("Downloaded ffmpeg is suspiciously small (<1 MB) — download may be corrupted. Please try again.".to_string());
        }
        let verify = tokio::process::Command::new(&ffmpeg_dest)
            .arg("-version")
            .output()
            .await;
        match verify {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.to_lowercase().contains("ffmpeg") {
                    let _ = std::fs::remove_file(&ffmpeg_dest);
                    return Err("Downloaded binary does not appear to be ffmpeg. Please try again.".to_string());
                }
                log::info!("ffmpeg integrity verified: {}", stdout.lines().next().unwrap_or("ok"));
            }
            _ => {
                let _ = std::fs::remove_file(&ffmpeg_dest);
                return Err("Downloaded ffmpeg failed execution test — the file may be corrupted. Please try again.".to_string());
            }
        }
    }

    let _ = app.emit("install-progress", serde_json::json!({
        "tool": "ffmpeg",
        "status": "completed",
        "progress": 100
    }));

    log::info!("ffmpeg installed successfully to {}", bin_dir.display());
    Ok(())
    }

}

/// Get diagnostic info about binary locations (useful for Android debugging)
#[tauri::command]
pub async fn get_binary_info(app: AppHandle) -> Result<serde_json::Value, String> {
    let ytdlp_path = download::get_ytdlp_path(&app);
    let ffmpeg_path = download::get_ffmpeg_path(&app);
    let bin_dir = download::get_binary_dir(&app);
    
    #[allow(unused_mut)]
    let mut info = serde_json::json!({
        "ytdlpPath": ytdlp_path,
        "ffmpegPath": ffmpeg_path,
        "binDir": bin_dir.to_string_lossy(),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    });
    
    #[cfg(target_os = "android")]
    {
        if let Some(native_dir) = download::get_native_lib_dir() {
            info["nativeLibDir"] = serde_json::json!(native_dir.to_string_lossy());
            
            // List files in nativeLibDir
            let mut native_files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&native_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    native_files.push(serde_json::json!({
                        "name": name,
                        "size": size,
                    }));
                }
            }
            info["nativeLibFiles"] = serde_json::json!(native_files);
        }
    }
    
    Ok(info)
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
        .timeout(std::time::Duration::from_secs(15))
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

/// Attempt to run `yt-dlp --version` through all available strategies and return
/// the version string along with which strategy worked.
#[tauri::command]
pub async fn probe_ytdlp(_app: AppHandle) -> Result<serde_json::Value, String> {
    // Strategy 1: bundled libytdlp.so (will likely fail on Android due to ELF interpreter)
    #[cfg(target_os = "android")]
    if let Some(native_dir) = download::get_native_lib_dir() {
        let path = native_dir.join("libytdlp.so");
        if path.exists() {
            match download::create_hidden_command(&path.to_string_lossy())
                .arg("--version").output().await {
                Ok(out) if out.status.success() => {
                    return Ok(serde_json::json!({
                        "strategy": "bundled",
                        "path": path.to_string_lossy(),
                        "version": String::from_utf8_lossy(&out.stdout).trim().to_string(),
                        "works": true,
                    }));
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    log::warn!("[probe_ytdlp] bundled libytdlp.so failed: {}", stderr.trim());
                    // Don't return yet — try Termux
                }
                Err(e) => {
                    let os_err = e.raw_os_error().unwrap_or(0);
                    let hint = if os_err == 2 {
                        " (ELF interpreter not found — Linux binaries can't run on Android)"
                    } else if os_err == 13 {
                        " (Permission denied)"
                    } else {
                        ""
                    };
                    log::warn!("[probe_ytdlp] bundled libytdlp.so exec error: {}{}", e, hint);
                }
            }
        }
    }

    // Strategy 2: Termux yt-dlp via RUN_COMMAND Intent
    #[cfg(target_os = "android")]
    {
        let (installed, has_perm) = crate::android_bridge::termux_info();
        if installed && has_perm {
            let check_dir = get_shared_check_dir();
            let output_file = format!("{}/probe_ytdlp.txt", check_dir);
            let _ = std::fs::remove_file(&output_file);

            if let Ok(true) = crate::android_bridge::run_termux_check("yt-dlp --version", &output_file) {
                for _ in 0..20 {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    if let Ok(content) = std::fs::read_to_string(&output_file) {
                        let trimmed = content.trim();
                        if !trimmed.is_empty() {
                            let _ = std::fs::remove_file(&output_file);
                            if trimmed.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                return Ok(serde_json::json!({
                                    "strategy": "termux",
                                    "path": "yt-dlp (Termux)",
                                    "version": trimmed,
                                    "works": true,
                                }));
                            } else {
                                return Ok(serde_json::json!({
                                    "strategy": "termux",
                                    "path": "yt-dlp (Termux)",
                                    "error": trimmed,
                                    "works": false,
                                }));
                            }
                        }
                    }
                }
            }
        }

        // All Android strategies failed
        return Ok(serde_json::json!({
            "strategy": "none",
            "path": "",
            "works": false,
            "error": if !installed {
                "Termux not installed. Linux binaries can't run directly on Android. Install Termux from F-Droid."
            } else if !has_perm {
                "Termux installed but RUN_COMMAND permission not granted. In Termux run: echo 'allow-external-apps=true' >> ~/.termux/termux.properties"
            } else {
                "yt-dlp not installed in Termux. Run: pkg install python && pip install yt-dlp"
            },
        }));
    }

    // Desktop: use resolved path
    #[cfg(not(target_os = "android"))]
    {
        let ytdlp = download::get_ytdlp_path(&_app);
        if let Ok(out) = download::create_hidden_command(&ytdlp)
            .arg("--version").output().await {
            if out.status.success() {
                return Ok(serde_json::json!({
                    "strategy": "resolved",
                    "path": ytdlp,
                    "version": String::from_utf8_lossy(&out.stdout).trim().to_string(),
                    "works": true,
                }));
            }
        }
        Ok(serde_json::json!({
            "strategy": "none",
            "path": ytdlp,
            "works": false,
            "error": "yt-dlp not found",
        }))
    }
}
