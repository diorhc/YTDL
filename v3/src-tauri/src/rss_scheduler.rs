use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tauri::{AppHandle, Manager, Emitter};

use crate::db::Database;
use crate::rss;

/// RSS background scheduler that periodically checks feeds for new content.
/// Uses `tokio::select!` with a `Notify` so interval changes take effect immediately
/// (Issue #9) and supports graceful shutdown via `AbortHandle` (Issue #10).
pub struct RssScheduler {
    is_running: Arc<Mutex<bool>>,
    interval_minutes: Arc<Mutex<u64>>,
    /// Notified whenever the interval changes or shutdown is requested,
    /// so the sleep loop wakes up immediately instead of waiting the full old interval.
    wake_notify: Arc<Notify>,
    /// Handle to abort the background task on shutdown.
    abort_handle: Mutex<Option<tokio::task::AbortHandle>>,
}

impl RssScheduler {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(Mutex::new(false)),
            interval_minutes: Arc::new(Mutex::new(60)), // Default 1 hour
            wake_notify: Arc::new(Notify::new()),
            abort_handle: Mutex::new(None),
        }
    }

    /// Start the background RSS checking task
    pub async fn start(&self, app: AppHandle) {
        let mut is_running = self.is_running.lock().await;
        if *is_running {
            return; // Already running
        }
        *is_running = true;
        drop(is_running);

        let is_running_clone = self.is_running.clone();
        let interval_minutes = self.interval_minutes.clone();
        let wake_notify = self.wake_notify.clone();

        let handle = tokio::spawn(async move {
            loop {
                // Check if still running BEFORE sleeping
                if !*is_running_clone.lock().await {
                    break;
                }

                // Get current interval
                let minutes = *interval_minutes.lock().await;
                if minutes == 0 {
                    // Disabled — wait for notification (interval change or shutdown)
                    wake_notify.notified().await;
                    continue;
                }

                let sleep_duration = Duration::from_secs(minutes * 60);

                // Use select! to wake up immediately when interval changes or shutdown
                tokio::select! {
                    _ = tokio::time::sleep(sleep_duration) => {
                        // Normal timeout — check feeds
                    }
                    _ = wake_notify.notified() => {
                        // Woken up by interval change or shutdown request
                        continue;
                    }
                }

                // Check if still running after sleep
                if !*is_running_clone.lock().await {
                    break;
                }

                // Check all feeds
                if let Err(e) = check_all_feeds(&app).await {
                    log::error!("RSS background check failed: {}", e);
                }
            }
            log::info!("[RssScheduler] Background task stopped");
        });

        // Store the abort handle for graceful shutdown
        let mut abort = self.abort_handle.lock().await;
        *abort = Some(handle.abort_handle());
    }

    /// Stop the background RSS checking task (graceful shutdown)
    pub async fn stop(&self) {
        let mut is_running = self.is_running.lock().await;
        *is_running = false;
        drop(is_running);

        // Wake the sleep so it exits the loop immediately
        self.wake_notify.notify_one();

        // Also abort the task as a safety net
        let mut abort = self.abort_handle.lock().await;
        if let Some(handle) = abort.take() {
            handle.abort();
        }
    }

    /// Set the check interval in minutes (0 to disable).
    /// Immediately wakes the scheduler so the new interval takes effect.
    pub async fn set_interval(&self, minutes: u64) {
        let mut interval = self.interval_minutes.lock().await;
        *interval = minutes;
        drop(interval);
        // Wake the scheduler so the new interval takes effect immediately
        self.wake_notify.notify_one();
    }

    /// Get the current interval in minutes
    pub async fn get_interval(&self) -> u64 {
        *self.interval_minutes.lock().await
    }
}

/// Check all RSS feeds and notify about new items
async fn check_all_feeds(app: &AppHandle) -> Result<(), String> {
    let db = app.state::<Arc<std::sync::Mutex<Database>>>();
    
    // Get all feeds
    let feeds = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock.get_feeds().map_err(|e| e.to_string())?
    };

    let mut new_items_count = 0;

    for feed in feeds {
        let feed_id = feed["id"].as_str().unwrap_or_default().to_string();
        let feed_url = feed["url"].as_str().unwrap_or_default().to_string();
        let feed_title = feed["channelName"].as_str().unwrap_or("Unknown").to_string();

        if feed_url.is_empty() {
            continue;
        }

        // Normalize and fetch
        let normalized_url = match rss::normalize_feed_url(&feed_url).await {
            Ok(url) => url,
            Err(e) => {
                log::warn!("Failed to normalize RSS URL {}: {}", feed_url, e);
                continue;
            }
        };

        let (title, items) = match rss::fetch_feed_items_extended(app, &normalized_url).await {
            Ok(result) => result,
            Err(e) => {
                log::warn!("Failed to fetch RSS feed {}: {}", feed_url, e);
                continue;
            }
        };

        // Update database
        {
            let db_lock = db.lock().map_err(|e| e.to_string())?;
            let _ = db_lock.update_feed_last_checked(&feed_id);
            if !title.is_empty() {
                let _ = db_lock.update_feed_channel_info(&feed_id, &title, "");
            }
            for item in &items {
                let already_exists = db_lock.feed_item_exists(&item.id);
                if db_lock.insert_feed_item(
                    &item.id,
                    &feed_id,
                    &item.video_id,
                    &item.title,
                    &item.thumbnail,
                    &item.url,
                    &item.published_at,
                    &item.video_type,
                ).is_ok() && !already_exists {
                    new_items_count += 1;
                }
            }
        }

        log::info!("Checked RSS feed: {} - {} items", feed_title, items.len());
    }

    if new_items_count > 0 {
        let _ = app.emit("rss-updated", serde_json::json!({
            "newItems": new_items_count
        }));

        if let Ok(db_lock) = db.lock() {
            if let Ok(Some(notifications)) = db_lock.get_setting("notifications") {
                if notifications == "true" {
                    #[cfg(desktop)]
                    {
                        use tauri_plugin_notification::NotificationExt;
                        let _ = app.notification()
                            .builder()
                            .title("New Videos Available")
                            .body(&format!("{} new videos from your subscriptions", new_items_count))
                            .show();
                    }
                }
            }
        }
    }

    Ok(())
}
