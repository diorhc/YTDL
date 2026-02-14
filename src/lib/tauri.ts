import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// --- Download types ---
export interface DownloadItem {
  id: string;
  url: string;
  title: string;
  thumbnail?: string;
  status: DownloadStatus;
  progress: number;
  speed?: string;
  eta?: string;
  filePath?: string;
  fileSize?: number;
  formatId?: string;
  formatLabel?: string;
  error?: string;
  priority?: number;
  source?: "single" | "playlist";
  createdAt: string;
  updatedAt?: string;
}

export type DownloadStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "completed"
  | "error"
  | "cancelled"
  | "merging";

export interface VideoInfo {
  id: string;
  title: string;
  thumbnail?: string;
  duration?: number;
  uploader?: string;
  url: string;
  formats: VideoFormat[];
}

export interface VideoFormat {
  formatId: string;
  ext: string;
  resolution: string;
  filesize?: number;
  vcodec: string;
  acodec: string;
  fps?: number;
  tbr?: number;
  formatNote: string;
  width?: number;
  height?: number;
}

// --- Playlist types ---
export interface PlaylistEntry {
  id: string;
  title: string;
  url: string;
  index: number;
  thumbnail?: string;
}

export interface PlaylistInfo {
  id: string;
  title: string;
  entries: PlaylistEntry[];
  entryCount: number;
}

export interface PlaylistDownloadOptions {
  url: string;
  startIndex?: number;
  endIndex?: number;
  format?: string;
  [key: string]: unknown; // Add index signature for Tauri invoke compatibility
}

// --- RSS types ---
export interface RssFeed {
  id: string;
  url: string;
  title: string;
  channelName?: string;
  channelAvatar?: string;
  lastChecked?: string;
  autoDownload: boolean;
  keywords: string[];
  ignoreKeywords: string[];
  items: RssItem[];
}

export interface RssItem {
  id: string;
  title: string;
  url: string;
  thumbnail?: string;
  publishedAt: string;
  status: "not_queued" | "queued" | "downloaded";
  videoType?: "video" | "short" | "unknown";
}

// --- Settings types ---
export interface StreamQuality {
  height: number;
  url: string;
  formatId: string;
  fps: number;
  ext: string;
}

export interface StreamInfo {
  videoUrl: string;
  audioUrl: string;
  combinedUrl: string;
  title: string;
  thumbnail: string;
  duration: number;
  uploader: string;
  qualities: StreamQuality[];
}

export interface AppSettings {
  downloadPath: string;
  maxConcurrentDownloads: number;
  speedLimit: number; // MB/s, 0 = unlimited
  autoStartDownloads: boolean;
  theme: "light" | "dark" | "system";
  language: string;
  notifications: boolean;
  closeToTray: boolean;
  autoLaunch: boolean;
  defaultQuality: string;
  qualityPreset: "best" | "4k" | "1080p" | "720p" | "audio"; // Quick quality presets
  defaultFormat: string;
  embedThumbnail: boolean;
  embedMetadata: boolean;
  browserForCookies: string;
  configPath: string;
  ytdlpFlags: string;
  // RSS settings
  rssCheckInterval: number;
  rssNotifications: boolean;
  rssAutoDownload: boolean;
}

// --- Transcript types ---
export interface TranscriptItem {
  id: string;
  source: string;
  title: string;
  language: string;
  text: string;
  status: string;
  progress: number;
  durationSecs: number;
  error: string;
  createdAt: string;
}

// --- Tauri commands ---
export const commands = {
  // Download commands
  startDownload: (url: string, formatId?: string) =>
    invoke<string>("start_download", { url, formatId }),
  pauseDownload: (id: string) => invoke<void>("pause_download", { id }),
  resumeDownload: (id: string) => invoke<void>("resume_download", { id }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  retryDownload: (id: string) => invoke<void>("retry_download", { id }),
  deleteDownload: (id: string, deleteFile: boolean) =>
    invoke<void>("delete_download", { id, deleteFile }),
  getDownloads: () => invoke<DownloadItem[]>("get_downloads"),
  getVideoInfo: (url: string) => invoke<VideoInfo>("get_video_info", { url }),
  getPlaylistInfo: (url: string) =>
    invoke<PlaylistInfo>("get_playlist_info", { url }),
  startPlaylistDownload: (options: PlaylistDownloadOptions) =>
    invoke<string[]>("start_playlist_download", options),

  // Batch download operations
  pauseAllDownloads: () => invoke<number>("pause_all_downloads"),
  resumeAllDownloads: () => invoke<number>("resume_all_downloads"),
  cancelAllDownloads: () => invoke<number>("cancel_all_downloads"),
  exportDownloads: (format: "json" | "csv") =>
    invoke<string>("export_downloads", { format }),

  // Settings commands
  getSettings: () => invoke<Record<string, string>>("get_settings"),
  saveSetting: (key: string, value: string) =>
    invoke<void>("save_setting", { key, value }),
  selectDirectory: () => invoke<string | null>("select_directory"),

  // RSS commands
  getFeeds: () => invoke<RssFeed[]>("get_feeds"),
  addFeed: (url: string) => invoke<string>("add_feed", { url }),
  removeFeed: (id: string) => invoke<void>("remove_feed", { id }),
  checkFeed: (id: string) => invoke<RssItem[]>("check_feed", { id }),
  checkAllRssFeeds: () => invoke<number>("check_all_rss_feeds"),
  markFeedItemWatched: (itemId: string, watched: boolean) =>
    invoke<void>("mark_feed_item_watched", { itemId, watched }),
  updateFeedSettings: (
    feedId: string,
    keywords: string,
    autoDownload: boolean,
  ) => invoke<void>("update_feed_settings", { feedId, keywords, autoDownload }),

  // RSS Scheduler commands
  setRssCheckInterval: (minutes: number) =>
    invoke<void>("set_rss_check_interval", { minutes }),
  getRssCheckInterval: () => invoke<number>("get_rss_check_interval"),

  // Stream proxy (custom player)
  getStreamUrl: (url: string) => invoke<StreamInfo>("get_stream_url", { url }),

  // Transcription commands
  startTranscription: (source: string, modelSize?: string) =>
    invoke<string>("start_transcription", { source, modelSize }),
  getTranscripts: () => invoke<TranscriptItem[]>("get_transcripts"),
  deleteTranscript: (id: string) => invoke<void>("delete_transcript", { id }),
  checkOpenaiTranscriptionApi: (apiKey: string, model: string) =>
    invoke<{ ok: boolean; model: string }>("check_openai_transcription_api", {
      apiKey,
      model,
    }),
  installLocalTranscription: (modelId: string) =>
    invoke<{
      ok: boolean;
      modelId: string;
      whisperCppPath: string;
      whisperModelPath: string;
    }>("install_local_transcription", { modelId }),

  // App commands
  getPlatform: () => invoke<string>("get_platform"),
  getAppVersion: () => invoke<string>("get_app_version"),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  checkYtdlp: () => invoke<boolean>("check_ytdlp"),
  checkFfmpeg: () => invoke<boolean>("check_ffmpeg"),
  installYtdlp: () => invoke<void>("install_ytdlp"),
  installFfmpeg: () => invoke<void>("install_ffmpeg"),
  getYtdlpVersion: () => invoke<string>("get_ytdlp_version"),
  getYtdlpLatestVersion: () => invoke<string>("get_ytdlp_latest_version"),
  updateYtdlp: () => invoke<void>("update_ytdlp"),
  getFfmpegVersion: () => invoke<string>("get_ffmpeg_version"),
  checkFfmpegUpdate: () => invoke<boolean>("check_ffmpeg_update"),
  updateFfmpeg: () => invoke<void>("update_ffmpeg"),

  // Priority commands
  setDownloadPriority: (downloadId: string, priority: number) =>
    invoke<void>("set_download_priority", { downloadId, priority }),
};

// --- Events ---
export interface DownloadProgressEvent {
  id: string;
  progress: number;
  speed?: string;
  eta?: string;
  status?: string;
}

export interface DownloadCompleteEvent {
  id: string;
  outputPath: string;
}

export interface DownloadErrorEvent {
  id: string;
  error: string;
}

export interface RssUpdatedEvent {
  newItems?: number;
  count?: number;
}

export interface RssSyncProgressEvent {
  feedId: string;
  phase: "fetching" | "importing" | "completed" | "error";
  processed: number;
  total: number;
  progress: number;
  message?: string;
}

export const events = {
  onDownloadProgress: (
    callback: (event: DownloadProgressEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<DownloadProgressEvent>("download-progress", (e) =>
      callback(e.payload),
    ),
  onDownloadComplete: (
    callback: (event: DownloadCompleteEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<DownloadCompleteEvent>("download-complete", (e) =>
      callback(e.payload),
    ),
  onDownloadError: (
    callback: (event: DownloadErrorEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<DownloadErrorEvent>("download-error", (e) => callback(e.payload)),
  onDownloadStatusChange: (
    callback: (event: { id: string; status: DownloadStatus }) => void,
  ): Promise<UnlistenFn> =>
    listen("download-status", (e) =>
      callback(e.payload as { id: string; status: DownloadStatus }),
    ),
  onRssUpdated: (
    callback: (event: RssUpdatedEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<RssUpdatedEvent>("rss-updated", (e) => callback(e.payload)),
  onRssSyncProgress: (
    callback: (event: RssSyncProgressEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<RssSyncProgressEvent>("rss-sync-progress", (e) =>
      callback(e.payload),
    ),
};
