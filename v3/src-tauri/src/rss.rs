use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::download;
use crate::error::{AppError, AppResult};

fn normalize_input_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    if trimmed.starts_with("www.")
        || trimmed.starts_with("youtube.com/")
        || trimmed.starts_with("www.youtube.com/")
    {
        return format!("https://{}", trimmed);
    }
    trimmed.to_string()
}

fn looks_like_youtube_url(url: &str) -> bool {
    url.contains("youtube.com/") || url.contains("www.youtube.com/")
}

fn channel_id_from_channel_url(url: &str) -> Option<String> {
    url.split("/channel/")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.split('?').next())
        .map(|s| s.to_string())
}

fn extract_channel_id_from_feed_url(feed_url: &str) -> Option<String> {
    if feed_url.contains("channel_id=") {
        feed_url
            .split("channel_id=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .map(|s| s.to_string())
    } else {
        None
    }
}

async fn resolve_youtube_channel_id(url: &str) -> AppResult<String> {
    use reqwest::header::CONTENT_TYPE;

    let client = reqwest::Client::builder()
        .user_agent("YTDL/3.0")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Rss(format!("HTTP client error: {}", e)))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Rss(format!("Failed to resolve YouTube channel: {}", e)))?;

    if let Some(cid) = channel_id_from_channel_url(response.url().as_str()) {
        return Ok(cid);
    }

    if let Some(ct) = response.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        if !ct.to_ascii_lowercase().contains("text/html") {
            return Err(AppError::Rss("Unexpected response type while resolving channel".to_string()));
        }
    }

    let body = response
        .text()
        .await
        .map_err(|e| AppError::Rss(format!("Failed to read YouTube page: {}", e)))?;

    let patterns = [
        r#""channelId":"(UC[0-9A-Za-z_-]{22})""#,
        r#"browseId":"(UC[0-9A-Za-z_-]{22})""#,
        r#"externalId":"(UC[0-9A-Za-z_-]{22})""#,
        r#"itemprop=\"channelId\"\s+content=\"(UC[0-9A-Za-z_-]{22})\""#,
    ];

    for pat in patterns {
        let re = regex::Regex::new(pat)
            .map_err(|e| AppError::Rss(format!("Regex error: {}", e)))?;
        if let Some(caps) = re.captures(&body) {
            if let Some(m) = caps.get(1) {
                return Ok(m.as_str().to_string());
            }
        }
    }

    Err(AppError::Rss(
        "Could not resolve YouTube channelId from the provided URL".to_string(),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RssFeed {
    pub id: String,
    pub url: String,
    pub title: String,
    pub channel_name: String,
    pub thumbnail: String,
    pub auto_download: bool,
    pub keywords: Vec<String>,
    pub last_checked: String,
    pub items: Vec<RssItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RssItem {
    pub id: String,
    pub title: String,
    pub video_id: String,
    pub url: String,
    pub thumbnail: String,
    pub published_at: String,
    pub downloaded: bool,
    pub video_type: String,
}

fn uploads_playlist_id(channel_id: &str) -> Option<String> {
    if channel_id.starts_with("UC") && channel_id.len() > 2 {
        Some(format!("UU{}", &channel_id[2..]))
    } else {
        None
    }
}

fn upload_date_to_iso(upload_date: &str) -> String {
    if upload_date.len() == 8 {
        let y = &upload_date[0..4];
        let m = &upload_date[4..6];
        let d = &upload_date[6..8];
        return format!("{}-{}-{}T00:00:00Z", y, m, d);
    }
    upload_date.to_string()
}

async fn run_ytdlp_json(ytdlp: &str, target_url: &str, playlist_end: &str) -> AppResult<serde_json::Value> {
    let output = download::create_hidden_command(ytdlp)
        .args([
            "-J",
            "--flat-playlist",
            "--no-warnings",
            "--skip-download",
            "--ignore-errors",
            "--playlist-end",
            playlist_end,
            target_url,
        ])
        .output()
        .await
        .map_err(|e| AppError::Rss(format!("Failed to execute yt-dlp: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(AppError::Rss(format!("yt-dlp failed: {}", stderr)));
    }

    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map_err(|e| AppError::Rss(format!("Failed to parse yt-dlp JSON: {}", e)))
}

fn entry_thumbnail(entry: &serde_json::Value, video_id: &str) -> String {
    entry["thumbnail"]
        .as_str()
        .or_else(|| {
            entry["thumbnails"]
                .as_array()
                .and_then(|t| t.last())
                .and_then(|t| t["url"].as_str())
        })
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", video_id))
}

async fn fetch_youtube_uploads_items(app: &AppHandle, channel_id: &str) -> AppResult<Vec<RssItem>> {
    let ytdlp = download::get_ytdlp_path(app);
    let shorts_url = format!("https://www.youtube.com/channel/{}/shorts", channel_id);
    let videos_url = format!("https://www.youtube.com/channel/{}/videos", channel_id);

    let mut short_ids = std::collections::HashSet::new();
    let mut all_items = Vec::new();

    if let Ok(shorts_json) = run_ytdlp_json(&ytdlp, &shorts_url, "5000").await {
        if let Some(entries) = shorts_json["entries"].as_array() {
            for entry in entries {
                let id = match entry["id"].as_str().or_else(|| entry["url"].as_str()) {
                    Some(v) => v.to_string(),
                    None => continue,
                };
                short_ids.insert(id.clone());

                all_items.push(RssItem {
                    id: id.clone(),
                    title: entry["title"].as_str().unwrap_or("Unknown").to_string(),
                    video_id: id.clone(),
                    url: format!("https://www.youtube.com/shorts/{}", id),
                    thumbnail: entry_thumbnail(entry, &id),
                    published_at: entry["upload_date"]
                        .as_str()
                        .map(upload_date_to_iso)
                        .unwrap_or_default(),
                    downloaded: false,
                    video_type: "short".to_string(),
                });
            }
        }
    }

    let videos_json = match run_ytdlp_json(&ytdlp, &videos_url, "5000").await {
        Ok(json) => Ok(json),
        Err(_) => {
            if let Some(uploads_id) = uploads_playlist_id(channel_id) {
                let playlist_url = format!("https://www.youtube.com/playlist?list={}", uploads_id);
                run_ytdlp_json(&ytdlp, &playlist_url, "5000").await
            } else {
                Err(AppError::Rss("No uploads playlist fallback available".to_string()))
            }
        }
    };

    if let Ok(json) = videos_json {
        if let Some(entries) = json["entries"].as_array() {
            for entry in entries {
                let id = match entry["id"].as_str().or_else(|| entry["url"].as_str()) {
                    Some(v) => v.to_string(),
                    None => continue,
                };

                let title = entry["title"].as_str().unwrap_or("Unknown").to_string();
                let marked_short = short_ids.contains(&id)
                    || title.to_lowercase().contains("#short")
                    || title.to_lowercase().contains("#shorts")
                    || entry["url"]
                        .as_str()
                        .map(|u| u.contains("/shorts/"))
                        .unwrap_or(false);

                let url = if marked_short {
                    format!("https://www.youtube.com/shorts/{}", id)
                } else {
                    format!("https://www.youtube.com/watch?v={}", id)
                };

                all_items.push(RssItem {
                    id: id.clone(),
                    title,
                    video_id: id.clone(),
                    url,
                    thumbnail: entry_thumbnail(entry, &id),
                    published_at: entry["upload_date"]
                        .as_str()
                        .map(upload_date_to_iso)
                        .unwrap_or_default(),
                    downloaded: false,
                    video_type: if marked_short {
                        "short".to_string()
                    } else {
                        "video".to_string()
                    },
                });
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    all_items.retain(|item| seen.insert(item.id.clone()));

    Ok(all_items)
}

pub async fn fetch_feed_items_extended(app: &AppHandle, feed_url: &str) -> AppResult<(String, Vec<RssItem>)> {
    let (mut title, mut items) = match fetch_feed_items(feed_url).await {
        Ok((t, i)) => (t, i),
        Err(e) => {
            log::warn!("RSS feed fetch failed for {}: {}", feed_url, e);
            (String::new(), Vec::new())
        }
    };

    if looks_like_youtube_url(feed_url) && feed_url.contains("feeds/videos.xml") {
        if let Some(channel_id) = extract_channel_id_from_feed_url(feed_url) {
            if let Ok(yt_items) = fetch_youtube_uploads_items(app, &channel_id).await {
                let mut map: std::collections::HashMap<String, RssItem> = yt_items
                    .into_iter()
                    .map(|item| (item.id.clone(), item))
                    .collect();

                for item in items.drain(..) {
                    if let Some(existing) = map.get_mut(&item.id) {
                        if !item.published_at.is_empty() {
                            existing.published_at = item.published_at;
                        }
                        if !item.thumbnail.is_empty() {
                            existing.thumbnail = item.thumbnail;
                        }
                    } else {
                        map.insert(item.id.clone(), item);
                    }
                }

                items = map.into_values().collect();
                if title.is_empty() {
                    title = format!("YouTube Channel {}", channel_id);
                }
            }
        }
    }

    items.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    Ok((title, items))
}

pub fn channel_to_rss_url(url: &str) -> AppResult<String> {
    if url.contains("youtube.com/feeds/videos.xml") {
        return Ok(url.to_string());
    }

    if url.contains("/channel/") {
        if let Some(id) = channel_id_from_channel_url(url) {
            return Ok(format!(
                "https://www.youtube.com/feeds/videos.xml?channel_id={}",
                id
            ));
        }
    }

    if url.ends_with(".xml") || url.contains("/feed") || url.contains("rss") {
        return Ok(url.to_string());
    }

    Err(AppError::Rss(
        "Please use a YouTube channel URL with /channel/UCXXX format or a direct RSS URL"
            .to_string(),
    ))
}

pub async fn normalize_feed_url(url: &str) -> AppResult<String> {
    let url = normalize_input_url(url);

    if url.contains("youtube.com/feeds/videos.xml")
        || url.ends_with(".xml")
        || url.contains("/feed")
        || url.contains("/rss")
        || url.contains("rsshub.app")
    {
        return Ok(url);
    }

    if looks_like_youtube_url(&url) {
        if url.contains("/channel/") {
            if let Some(id) = channel_id_from_channel_url(&url) {
                return Ok(format!(
                    "https://www.youtube.com/feeds/videos.xml?channel_id={}",
                    id
                ));
            }
        }

        if url.contains("/@") || url.contains("/user/") || url.contains("/c/") {
            let channel_id = resolve_youtube_channel_id(&url).await?;
            return Ok(format!(
                "https://www.youtube.com/feeds/videos.xml?channel_id={}",
                channel_id
            ));
        }

        let channel_id = resolve_youtube_channel_id(&url).await?;
        return Ok(format!(
            "https://www.youtube.com/feeds/videos.xml?channel_id={}",
            channel_id
        ));
    }

    Ok(url)
}

pub async fn fetch_feed_items(feed_url: &str) -> AppResult<(String, Vec<RssItem>)> {
    use reqwest::header::CONTENT_TYPE;

    let client = reqwest::Client::builder()
        .user_agent("YTDL/3.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Rss(format!("HTTP client error: {}", e)))?;

    let response = client
        .get(feed_url)
        .send()
        .await
        .map_err(|e| AppError::Rss(format!("Failed to fetch feed: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Rss(format!(
            "Feed returned status {}",
            response.status()
        )));
    }

    if let Some(ct) = response.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        if ct.to_ascii_lowercase().contains("text/html") {
            return Err(AppError::Rss(
                "URL does not appear to be a feed (server returned HTML)".to_string(),
            ));
        }
    }

    let body = response
        .text()
        .await
        .map_err(|e| AppError::Rss(format!("Failed to read response: {}", e)))?;

    parse_atom_feed(&body)
}

async fn fetch_youtube_channel_avatar(channel_id: &str) -> Option<String> {
    let channel_url = format!("https://www.youtube.com/channel/{}", channel_id);

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let response = client.get(&channel_url).send().await.ok()?;
    let body = response.text().await.ok()?;

    let patterns = [
        r#""avatar":\{"thumbnails":\[\{"url":"([^"]+)""#,
        r#""channelMetadataRenderer":\{[^}]*"avatar":\{"thumbnails":\[\{"url":"([^"]+)""#,
        r#"property="og:image"\s+content="([^"]+)""#,
    ];

    for pat in patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            if let Some(caps) = re.captures(&body) {
                if let Some(m) = caps.get(1) {
                    let url = m
                        .as_str()
                        .replace("\\u0026", "&")
                        .replace("\\u003d", "=")
                        .replace("\\/", "/");
                    return Some(url);
                }
            }
        }
    }

    None
}

async fn fetch_youtube_channel_avatar_via_ytdlp(app: &AppHandle, channel_id: &str) -> Option<String> {
    let ytdlp = download::get_ytdlp_path(app);
    let videos_url = format!("https://www.youtube.com/channel/{}/videos", channel_id);

    let json = run_ytdlp_json(&ytdlp, &videos_url, "1").await.ok()?;

    if let Some(url) = json["channel_thumbnail"].as_str() {
        return Some(url.to_string());
    }
    if let Some(url) = json["thumbnail"].as_str() {
        return Some(url.to_string());
    }

    json["thumbnails"]
        .as_array()
        .and_then(|arr| arr.last())
        .and_then(|thumb| thumb["url"].as_str())
        .map(|u| u.to_string())
}

pub async fn get_channel_avatar(feed_url: &str) -> Option<String> {
    if let Some(channel_id) = extract_channel_id_from_feed_url(feed_url) {
        fetch_youtube_channel_avatar(&channel_id).await
    } else {
        None
    }
}

pub async fn get_channel_avatar_with_fallback(app: &AppHandle, feed_url: &str) -> Option<String> {
    if let Some(avatar) = get_channel_avatar(feed_url).await {
        if !avatar.trim().is_empty() {
            return Some(avatar);
        }
    }

    if let Some(channel_id) = extract_channel_id_from_feed_url(feed_url) {
        return fetch_youtube_channel_avatar_via_ytdlp(app, &channel_id).await;
    }

    None
}

fn parse_atom_feed(xml: &str) -> AppResult<(String, Vec<RssItem>)> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut items = Vec::new();
    let mut feed_title = String::new();
    let mut buf = Vec::new();

    let mut in_entry = false;
    let mut in_title = false;
    let mut in_published = false;
    let mut in_feed_title = false;

    let mut current_title = String::new();
    let mut current_video_id = String::new();
    let mut current_url = String::new();
    let mut current_published = String::new();
    let mut current_thumbnail = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "entry" => {
                        in_entry = true;
                        current_title.clear();
                        current_video_id.clear();
                        current_url.clear();
                        current_published.clear();
                        current_thumbnail.clear();
                    }
                    "title" => {
                        if in_entry {
                            in_title = true;
                        } else {
                            in_feed_title = true;
                        }
                    }
                    "published" => {
                        if in_entry {
                            in_published = true;
                        }
                    }
                    "yt:videoId" => {
                        if let Ok(Event::Text(text)) = reader.read_event_into(&mut buf) {
                            current_video_id = text.unescape().unwrap_or_default().to_string();
                            current_url = format!(
                                "https://www.youtube.com/watch?v={}",
                                current_video_id
                            );
                            current_thumbnail = format!(
                                "https://i.ytimg.com/vi/{}/mqdefault.jpg",
                                current_video_id
                            );
                        }
                    }
                    "link" => {
                        if in_entry {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"href" {
                                    current_url = String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        }
                    }
                    "media:thumbnail" => {
                        if in_entry {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"url" {
                                    current_thumbnail =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                if in_title {
                    current_title = text;
                } else if in_feed_title {
                    feed_title = text;
                } else if in_published {
                    current_published = text;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "entry" => {
                        if !current_video_id.is_empty() {
                            let video_type = if current_url.contains("/shorts/")
                                || current_title.to_lowercase().contains("#short")
                                || current_title.to_lowercase().contains("#shorts")
                            {
                                "short"
                            } else {
                                "video"
                            };

                            items.push(RssItem {
                                id: current_video_id.clone(),
                                title: current_title.clone(),
                                video_id: current_video_id.clone(),
                                url: current_url.clone(),
                                thumbnail: current_thumbnail.clone(),
                                published_at: current_published.clone(),
                                downloaded: false,
                                video_type: video_type.to_string(),
                            });
                        }
                        in_entry = false;
                    }
                    "title" => {
                        in_title = false;
                        in_feed_title = false;
                    }
                    "published" => {
                        in_published = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => continue,
            _ => {}
        }
        buf.clear();
    }

    Ok((feed_title, items))
}
