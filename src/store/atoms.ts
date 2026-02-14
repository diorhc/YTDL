import { atom } from "jotai";
import type {
  DownloadItem,
  AppSettings,
  RssFeed,
  VideoInfo,
} from "@/lib/tauri";

// ─── Download State ─────────────────────────────────────
export const downloadsAtom = atom<DownloadItem[]>([]);
export const downloadLoadingAtom = atom(false);

// ─── Current video info (for quality selection dialog) ──
export const videoInfoAtom = atom<VideoInfo | null>(null);
export const showQualityDialogAtom = atom(false);
export const pendingUrlAtom = atom("");

// ─── RSS State ──────────────────────────────────────────
export const feedsAtom = atom<RssFeed[]>([]);
export const feedsLoadingAtom = atom(false);

// ─── Settings State ─────────────────────────────────────
export const settingsAtom = atom<AppSettings>({
  downloadPath: "",
  maxConcurrentDownloads: 3,
  speedLimit: 0, // 0 = unlimited
  autoStartDownloads: true,
  theme: "system",
  language: "en",
  notifications: true,
  closeToTray: false,
  autoLaunch: false,
  defaultQuality: "best",
  qualityPreset: "best",
  defaultFormat: "mp4",
  embedThumbnail: true,
  embedMetadata: true,
  browserForCookies: "none",
  configPath: "",
  ytdlpFlags: "",
  rssCheckInterval: 60,
  rssNotifications: true,
  rssAutoDownload: false,
});
export const settingsLoadedAtom = atom(false);

// ─── Transcription state ────────────────────────────────
export interface TranscriptItem {
  id: string;
  title: string;
  source: "url" | "file";
  status: "pending" | "processing" | "completed" | "error";
  progress: number;
  text?: string;
  language?: string;
  duration?: string;
  createdAt: string;
  error?: string;
}
export const transcriptsAtom = atom<TranscriptItem[]>([]);

// ─── Platform ───────────────────────────────────────────
export const platformAtom = atom("");
export const appVersionAtom = atom("");
