use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Log levels for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

/// Application logger that writes to a file
pub struct AppLogger {
    file: Mutex<Option<File>>,
    path: PathBuf,
    min_level: LogLevel,
}

impl AppLogger {
    pub fn new(log_dir: &std::path::Path, min_level: LogLevel) -> Self {
        let date = Local::now().format("%Y-%m-%d").to_string();
        let path = log_dir.join(format!("ytdl-{}.log", date));

        // Ensure directory exists
        std::fs::create_dir_all(log_dir).ok();

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();

        Self {
            file: Mutex::new(file),
            path,
            min_level,
        }
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        if (level as u8) < (self.min_level as u8) {
            return;
        }

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let line = format!("[{}] [{}] {}\n", timestamp, level.as_str(), message);

        // Write to file
        if let Ok(mut file_lock) = self.file.lock() {
            if let Some(file) = file_lock.as_mut() {
                let _ = file.write_all(line.as_bytes());
            }
        }

        // Also write to stderr for dev
        #[cfg(debug_assertions)]
        eprintln!("{}", line.trim());
    }

    pub fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.path
    }
}

/// Convenience macros for logging
#[macro_export]
macro_rules! app_debug {
    ($logger:expr, $($arg:tt)*) => {
        $logger.debug(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! app_info {
    ($logger:expr, $($arg:tt)*) => {
        $logger.info(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! app_warn {
    ($logger:expr, $($arg:tt)*) => {
        $logger.warn(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! app_error {
    ($logger:expr, $($arg:tt)*) => {
        $logger.error(&format!($($arg)*))
    };
}
