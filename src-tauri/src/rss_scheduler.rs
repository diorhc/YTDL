use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::interval;
use tauri::{AppHandle, Manager, Emitter};

use crate::db::Database;
use crate::rss;

/// RSS background scheduler that periodically checks feeds for new content
pub struct RssScheduler {
    is_running: Arc<Mutex<bool>>,
    interval_minutes: Arc<Mutex<u64>>,
}

impl RssScheduler {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(Mutex::new(false)),
            interval_minutes: Arc::new(Mutex::new(60)), // Default 1 hour
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

        tokio::spawn(async move {
            loop {
                // Get current interval
                let minutes = *interval_minutes.lock().await;
                if minutes == 0 {
                    // Disabled, check again in 1 minute
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }

                // Wait for the interval
                let mut ticker = interval(Duration::from_secs(minutes * 60));
                ticker.tick().await; // First tick is immediate, skip it
                ticker.tick().await; // Wait for actual interval

                // Check if still running
                if !*is_running_clone.lock().await {
                    break;
                }

                // Check all feeds
                if let Err(e) = check_all_feeds(&app).await {
                    log::error!("RSS background check failed: {}", e);
                }
            }
        });
    }

    /// Stop the background RSS checking task
    pub async fn stop(&self) {
        let mut is_running = self.is_running.lock().await;
        *is_running = false;
    }

    /// Set the check interval in minutes (0 to disable)
    pub async fn set_interval(&self, minutes: u64) {
        let mut interval = self.interval_minutes.lock().await;
        *interval = minutes;
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
            
            // Update last checked
            let _ = db_lock.update_feed_last_checked(&feed_id);
            
            // Update channel info
            if !title.is_empty() {
                let _ = db_lock.update_feed_channel_info(&feed_id, &title, "");
            }

            // Insert new items (INSERT OR IGNORE will skip existing)
            for item in &items {
                if db_lock.insert_feed_item(
                    &item.id,
                    &feed_id,
                    &item.video_id,
                    &item.title,
                    &item.thumbnail,
                    &item.url,
                    &item.published_at,
                    &item.video_type,
                ).is_ok() {
                    new_items_count += 1;
                }
            }
        }

        log::info!("Checked RSS feed: {} - {} items", feed_title, items.len());
    }

    // Send notification if new items found
    if new_items_count > 0 {
        // Emit event to frontend
        let _ = app.emit("rss-updated", serde_json::json!({
            "newItems": new_items_count
        }));

        // Show desktop notification if enabled
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
