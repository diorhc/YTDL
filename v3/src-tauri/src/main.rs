#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Logging is initialized inside run() (env_logger for desktop, android_logger for Android)
    ytdl_lib::run();
}
