use rusqlite::{params, Connection};
use std::path::Path;

use crate::error::AppResult;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: &Path) -> AppResult<Self> {
        println!("[DB] Opening database at: {:?}", path);
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    pub fn migrate(&self) -> AppResult<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                thumbnail TEXT DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                progress REAL NOT NULL DEFAULT 0.0,
                speed TEXT DEFAULT '',
                eta TEXT DEFAULT '',
                file_path TEXT DEFAULT '',
                file_size INTEGER DEFAULT 0,
                format_id TEXT DEFAULT '',
                format_label TEXT DEFAULT '',
                error TEXT DEFAULT '',
                priority INTEGER DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS feeds (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                channel_name TEXT DEFAULT '',
                thumbnail TEXT DEFAULT '',
                auto_download INTEGER NOT NULL DEFAULT 0,
                keywords TEXT DEFAULT '[]',
                last_checked TEXT DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS feed_items (
                id TEXT PRIMARY KEY,
                feed_id TEXT NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
                video_id TEXT NOT NULL,
                title TEXT NOT NULL,
                thumbnail TEXT DEFAULT '',
                url TEXT DEFAULT '',
                published_at TEXT DEFAULT '',
                downloaded INTEGER NOT NULL DEFAULT 0,
                video_type TEXT DEFAULT 'video'
            );

            CREATE TABLE IF NOT EXISTS transcripts (
                id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                language TEXT DEFAULT '',
                text TEXT DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                progress REAL NOT NULL DEFAULT 0.0,
                duration_secs INTEGER DEFAULT 0,
                error TEXT DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS playlists (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                thumbnail TEXT DEFAULT '',
                total_videos INTEGER DEFAULT 0,
                downloaded_videos INTEGER DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'pending',
                naming_template TEXT DEFAULT '%(title)s.%(ext)s',
                auto_sync INTEGER DEFAULT 0,
                last_sync TEXT DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Default settings
            INSERT OR IGNORE INTO settings (key, value) VALUES ('theme', 'system');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('language', 'en');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('notifications', 'true');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('close_to_tray', 'false');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('auto_launch', 'false');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('auto_start_download', 'true');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('embed_thumbnail', 'true');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('embed_metadata', 'true');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('browser_cookies', 'none');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('ytdlp_flags', '');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('transcribe_provider', 'api');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('openai_api_key', '');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('openai_model', 'whisper-1');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('whisper_cpp_path', '');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('whisper_model_path', '');
            ",
        )?;
        
        // Migrations: Add video_type column if it doesn't exist
        // This is safe to run multiple times
        match self.conn.execute(
            "ALTER TABLE feed_items ADD COLUMN video_type TEXT DEFAULT 'video'",
            [],
        ) {
            Ok(_) => println!("[DB] Added video_type column to feed_items"),
            Err(e) => println!("[DB] video_type column already exists or error: {}", e),
        }

        // Legacy schema compatibility migrations for feed_items
        match self.conn.execute(
            "ALTER TABLE feed_items ADD COLUMN thumbnail TEXT DEFAULT ''",
            [],
        ) {
            Ok(_) => println!("[DB] Added thumbnail column to feed_items"),
            Err(e) => println!("[DB] thumbnail column already exists or error: {}", e),
        }
        match self.conn.execute(
            "ALTER TABLE feed_items ADD COLUMN url TEXT DEFAULT ''",
            [],
        ) {
            Ok(_) => println!("[DB] Added url column to feed_items"),
            Err(e) => println!("[DB] url column already exists or error: {}", e),
        }

        self.conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_feed_items_feed_id_published
            ON feed_items(feed_id, published_at DESC);

            CREATE INDEX IF NOT EXISTS idx_feed_items_video_id
            ON feed_items(video_id);

            CREATE INDEX IF NOT EXISTS idx_feeds_created_at
            ON feeds(created_at DESC);
            ",
        )?;

        // Migration: Add source column to downloads table
        match self.conn.execute(
            "ALTER TABLE downloads ADD COLUMN source TEXT DEFAULT 'single'",
            [],
        ) {
            Ok(_) => println!("[DB] Added source column to downloads"),
            Err(e) => println!("[DB] source column already exists or error: {}", e),
        }
        
        Ok(())
    }

    // --- Downloads ---

    pub fn insert_download(
        &self,
        id: &str,
        url: &str,
        title: &str,
        thumbnail: &str,
    ) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO downloads (id, url, title, thumbnail) VALUES (?1, ?2, ?3, ?4)",
            params![id, url, title, thumbnail],
        )?;
        Ok(())
    }

    pub fn insert_download_with_source(
        &self,
        id: &str,
        url: &str,
        title: &str,
        thumbnail: &str,
        source: &str,
    ) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO downloads (id, url, title, thumbnail, source) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, url, title, thumbnail, source],
        )?;
        Ok(())
    }

    pub fn update_download_status(&self, id: &str, status: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE downloads SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, status],
        )?;
        Ok(())
    }

    pub fn update_download_progress(
        &self,
        id: &str,
        progress: f64,
        speed: &str,
        eta: &str,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE downloads SET progress = ?2, speed = ?3, eta = ?4, updated_at = datetime('now') WHERE id = ?1",
            params![id, progress, speed, eta],
        )?;
        Ok(())
    }

    pub fn update_download_complete(
        &self,
        id: &str,
        file_path: &str,
        file_size: i64,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE downloads SET status = 'completed', progress = 100.0, file_path = ?2, file_size = ?3, updated_at = datetime('now') WHERE id = ?1",
            params![id, file_path, file_size],
        )?;
        Ok(())
    }

    pub fn update_download_error(&self, id: &str, error: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE downloads SET status = 'error', error = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, error],
        )?;
        Ok(())
    }

    pub fn delete_download(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn update_download_priority(&self, id: &str, priority: i32) -> AppResult<()> {
        self.conn.execute(
            "UPDATE downloads SET priority = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, priority],
        )?;
        Ok(())
    }

    pub fn get_download_priority(&self, id: &str) -> AppResult<i32> {
        let mut stmt = self
            .conn
            .prepare("SELECT priority FROM downloads WHERE id = ?1")?;
        let priority = stmt.query_row(params![id], |row| row.get(0)).unwrap_or(0);
        Ok(priority)
    }

    pub fn get_downloads(&self) -> AppResult<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, thumbnail, status, progress, speed, eta, file_path, file_size, format_id, format_label, error, priority, created_at, updated_at, COALESCE(source, 'single') FROM downloads ORDER BY priority DESC, created_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "url": row.get::<_, String>(1)?,
                "title": row.get::<_, String>(2)?,
                "thumbnail": row.get::<_, String>(3)?,
                "status": row.get::<_, String>(4)?,
                "progress": row.get::<_, f64>(5)?,
                "speed": row.get::<_, String>(6)?,
                "eta": row.get::<_, String>(7)?,
                "filePath": row.get::<_, String>(8)?,
                "fileSize": row.get::<_, i64>(9)?,
                "formatId": row.get::<_, String>(10)?,
                "formatLabel": row.get::<_, String>(11)?,
                "error": row.get::<_, String>(12)?,
                "priority": row.get::<_, i32>(13).unwrap_or(0),
                "createdAt": row.get::<_, String>(14)?,
                "updatedAt": row.get::<_, String>(15)?,
                "source": row.get::<_, String>(16).unwrap_or_else(|_| "single".to_string()),
            }))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // --- Settings ---

    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;
        let result = stmt.query_row(params![key], |row| row.get(0)).ok();
        Ok(result)
    }

    pub fn get_all_settings(&self) -> AppResult<serde_json::Value> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut map = serde_json::Map::new();
        for row in rows {
            let (key, value) = row?;
            map.insert(key, serde_json::Value::String(value));
        }
        Ok(serde_json::Value::Object(map))
    }

    pub fn save_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    // --- Feeds ---

    pub fn insert_feed(&self, id: &str, url: &str, title: &str, thumbnail: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO feeds (id, url, title, channel_name, thumbnail) VALUES (?1, ?2, ?3, '', ?4)",
            params![id, url, title, thumbnail],
        )?;
        Ok(())
    }

    pub fn get_feeds(&self) -> AppResult<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, channel_name, thumbnail, auto_download, keywords, last_checked, created_at FROM feeds ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (
                id,
                url,
                title,
                channel_name,
                thumbnail,
                auto_download,
                keywords,
                last_checked,
                created_at,
            ) = row?;
            // Get items for this feed
            let items = self.get_feed_items(&id).unwrap_or_default();
            result.push(serde_json::json!({
                "id": id,
                "url": url,
                "title": title,
                "channelName": channel_name,
                "channelAvatar": thumbnail,
                "autoDownload": auto_download,
                "keywords": keywords,
                "lastChecked": last_checked,
                "createdAt": created_at,
                "items": items,
            }));
        }
        Ok(result)
    }

    pub fn delete_feed(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM feeds WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn update_feed_last_checked(&self, id: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE feeds SET last_checked = datetime('now') WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn update_feed_url(&self, id: &str, url: &str) -> AppResult<()> {
        self.conn
            .execute("UPDATE feeds SET url = ?2 WHERE id = ?1", params![id, url])?;
        Ok(())
    }

    pub fn update_feed_channel_info(
        &self,
        id: &str,
        channel_name: &str,
        thumbnail: &str,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE feeds SET channel_name = ?2, thumbnail = ?3 WHERE id = ?1",
            params![id, channel_name, thumbnail],
        )?;
        Ok(())
    }

    pub fn update_feed_settings(
        &self,
        id: &str,
        keywords: &str,
        auto_download: bool,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE feeds SET keywords = ?2, auto_download = ?3 WHERE id = ?1",
            params![id, keywords, auto_download as i32],
        )?;
        Ok(())
    }

    // --- Feed Items ---

    pub fn insert_feed_item(
        &self,
        id: &str,
        feed_id: &str,
        video_id: &str,
        title: &str,
        thumbnail: &str,
        url: &str,
        published_at: &str,
        video_type: &str,
    ) -> AppResult<()> {
                let result = self.conn.execute(
                        "INSERT INTO feed_items (id, feed_id, video_id, title, thumbnail, url, published_at, video_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                         ON CONFLICT(id) DO UPDATE SET \
                             feed_id = excluded.feed_id, \
                             video_id = excluded.video_id, \
                             title = excluded.title, \
                             thumbnail = excluded.thumbnail, \
                             url = excluded.url, \
                             published_at = excluded.published_at, \
                             video_type = excluded.video_type",
            params![id, feed_id, video_id, title, thumbnail, url, published_at, video_type],
        );

        if result.is_err() {
                        let fallback_with_thumb_url = self.conn.execute(
                                "INSERT INTO feed_items (id, feed_id, video_id, title, thumbnail, url, published_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                                 ON CONFLICT(id) DO UPDATE SET \
                                     feed_id = excluded.feed_id, \
                                     video_id = excluded.video_id, \
                                     title = excluded.title, \
                                     thumbnail = excluded.thumbnail, \
                                     url = excluded.url, \
                                     published_at = excluded.published_at",
                params![id, feed_id, video_id, title, thumbnail, url, published_at],
            );

            if fallback_with_thumb_url.is_err() {
                                self.conn.execute(
                                        "INSERT INTO feed_items (id, feed_id, video_id, title, published_at) VALUES (?1, ?2, ?3, ?4, ?5) \
                                         ON CONFLICT(id) DO UPDATE SET \
                                             feed_id = excluded.feed_id, \
                                             video_id = excluded.video_id, \
                                             title = excluded.title, \
                                             published_at = excluded.published_at",
                    params![id, feed_id, video_id, title, published_at],
                )?;
            }
        }
        Ok(())
    }

    pub fn get_feed_items(&self, feed_id: &str) -> AppResult<Vec<serde_json::Value>> {
        let query_with_type =
            "SELECT id, video_id, title, thumbnail, url, published_at, downloaded, video_type FROM feed_items WHERE feed_id = ?1 ORDER BY published_at DESC";

        let mut result = Vec::new();

        match self.conn.prepare(query_with_type) {
            Ok(mut stmt) => {
                let rows = stmt.query_map(params![feed_id], |row| {
                    let downloaded_raw: i64 = row.get::<_, i64>(6).unwrap_or(0);
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "videoId": row.get::<_, String>(1)?,
                        "title": row.get::<_, String>(2)?,
                        "thumbnail": row.get::<_, String>(3)?,
                        "url": row.get::<_, String>(4)?,
                        "publishedAt": row.get::<_, String>(5)?,
                        "status": if downloaded_raw != 0 { "downloaded" } else { "not_queued" },
                        "videoType": row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "video".to_string()),
                    }))
                })?;
                for row in rows {
                    result.push(row?);
                }
                Ok(result)
            }
            Err(e) => {
                println!("[DB] feed_items full-schema read failed, using fallback: {:?}", e);
                let with_thumb_url = self.conn.prepare(
                    "SELECT id, video_id, title, thumbnail, url, published_at, downloaded FROM feed_items WHERE feed_id = ?1 ORDER BY published_at DESC",
                );

                if let Ok(mut stmt) = with_thumb_url {
                    let rows = stmt.query_map(params![feed_id], |row| {
                        let downloaded_raw: i64 = row.get::<_, i64>(6).unwrap_or(0);
                        let url = row.get::<_, String>(4)?;
                        let title = row.get::<_, String>(2)?;
                        let inferred = if url.to_lowercase().contains("/shorts/")
                            || title.to_lowercase().contains("#short")
                            || title.to_lowercase().contains("#shorts")
                        {
                            "short"
                        } else {
                            "video"
                            };

                        Ok(serde_json::json!({
                            "id": row.get::<_, String>(0)?,
                            "videoId": row.get::<_, String>(1)?,
                            "thumbnail": row.get::<_, String>(3)?,
                            "url": url,
                            "publishedAt": row.get::<_, String>(5)?,
                            "status": if downloaded_raw != 0 { "downloaded" } else { "not_queued" },
                            "videoType": inferred,
                        }))
                    })?;
                    for row in rows {
                        result.push(row?);
                    }
                    println!("[DB] Read {} items with thumb/url fallback", result.len());
                    return Ok(result);
                }
                println!("[DB] Using minimal schema fallback read");
                let mut stmt = self.conn.prepare(
                    "SELECT id, video_id, title, published_at, downloaded FROM feed_items WHERE feed_id = ?1 ORDER BY published_at DESC",
                )?;
                let rows = stmt.query_map(params![feed_id], |row| {
                    let downloaded_raw: i64 = row.get::<_, i64>(4).unwrap_or(0);
                    let video_id = row.get::<_, String>(1)?;
                    let title = row.get::<_, String>(2)?;
                    let inferred = if title.to_lowercase().contains("#short")
                        || title.to_lowercase().contains("#shorts")
                    {
                        "short"
                    } else {
                        "video"
                    };
                    let url = if inferred == "short" {
                        format!("https://www.youtube.com/shorts/{}", video_id)
                    } else {
                        format!("https://www.youtube.com/watch?v={}", video_id)
                    };

                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "videoId": video_id.clone(),
                        "title": title,
                        "thumbnail": format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", video_id),
                        "url": url,
                        "publishedAt": row.get::<_, String>(3)?,
                        "status": if downloaded_raw != 0 { "downloaded" } else { "not_queued" },
                        "videoType": inferred,
                    }))
                })?;
                for row in rows {
                    result.push(row?);
                }
                println!("[DB] Read {} items with minimal schema", result.len());
                Ok(result)
            }
        }
    }

    pub fn update_feed_item_downloaded(&self, id: &str, downloaded: bool) -> AppResult<()> {
        self.conn.execute(
            "UPDATE feed_items SET downloaded = ?2 WHERE id = ?1",
            params![id, downloaded],
        )?;
        Ok(())
    }

    // --- Transcripts ---

    pub fn insert_transcript(&self, id: &str, source: &str, title: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO transcripts (id, source, title) VALUES (?1, ?2, ?3)",
            params![id, source, title],
        )?;
        Ok(())
    }

    pub fn get_transcripts(&self) -> AppResult<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source, title, language, text, status, progress, duration_secs, error, created_at FROM transcripts ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "source": row.get::<_, String>(1)?,
                "title": row.get::<_, String>(2)?,
                "language": row.get::<_, String>(3)?,
                "text": row.get::<_, String>(4)?,
                "status": row.get::<_, String>(5)?,
                "progress": row.get::<_, f64>(6)?,
                "durationSecs": row.get::<_, i64>(7)?,
                "error": row.get::<_, String>(8)?,
                "createdAt": row.get::<_, String>(9)?,
            }))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn update_transcript_status(&self, id: &str, status: &str, progress: f64) -> AppResult<()> {
        self.conn.execute(
            "UPDATE transcripts SET status = ?2, progress = ?3 WHERE id = ?1",
            params![id, status, progress],
        )?;
        Ok(())
    }

    pub fn update_transcript_complete(
        &self,
        id: &str,
        text: &str,
        language: &str,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE transcripts SET status = 'completed', progress = 100.0, text = ?2, language = ?3 WHERE id = ?1",
            params![id, text, language],
        )?;
        Ok(())
    }

    pub fn update_transcript_error(&self, id: &str, error: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE transcripts SET status = 'error', error = ?2 WHERE id = ?1",
            params![id, error],
        )?;
        Ok(())
    }

    pub fn delete_transcript(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM transcripts WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Playlists ---

    pub fn insert_playlist(
        &self,
        id: &str,
        url: &str,
        title: &str,
        total_videos: i32,
    ) -> AppResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO playlists (id, url, title, total_videos, status, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, 'downloading', datetime('now'), datetime('now'))",
            params![id, url, title, total_videos],
        )?;
        Ok(())
    }

    pub fn update_playlist_progress(&self, id: &str, downloaded_videos: i32) -> AppResult<()> {
        self.conn.execute(
            "UPDATE playlists SET downloaded_videos = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, downloaded_videos],
        )?;
        Ok(())
    }

    pub fn update_playlist_settings(
        &self,
        id: &str,
        naming_template: &str,
        auto_sync: bool,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE playlists SET naming_template = ?2, auto_sync = ?3, updated_at = datetime('now') WHERE id = ?1",
            params![id, naming_template, auto_sync as i32],
        )?;
        Ok(())
    }

    pub fn get_playlists(&self) -> AppResult<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, thumbnail, total_videos, downloaded_videos, status, naming_template, auto_sync, last_sync, created_at, updated_at FROM playlists ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "url": row.get::<_, String>(1)?,
                "title": row.get::<_, String>(2)?,
                "thumbnail": row.get::<_, String>(3)?,
                "totalVideos": row.get::<_, i32>(4)?,
                "downloadedVideos": row.get::<_, i32>(5)?,
                "status": row.get::<_, String>(6)?,
                "namingTemplate": row.get::<_, String>(7)?,
                "autoSync": row.get::<_, i32>(8)? != 0,
                "lastSync": row.get::<_, String>(9)?,
                "createdAt": row.get::<_, String>(10)?,
                "updatedAt": row.get::<_, String>(11)?,
            }))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn delete_playlist(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
        Ok(())
    }
}
