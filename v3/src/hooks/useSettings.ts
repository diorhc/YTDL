import { useAtom } from "jotai";
import { useCallback, useEffect, useRef } from "react";
import { commands } from "@/lib/tauri";
import { settingsAtom, settingsLoadedAtom } from "@/store/atoms";
import { useTheme } from "next-themes";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

export function useSettings() {
  const [settings, setSettings] = useAtom(settingsAtom);
  const [loaded, setLoaded] = useAtom(settingsLoadedAtom);
  const { setTheme } = useTheme();
  const { i18n } = useTranslation();
  const initialized = useRef(false);

  const loadSettings = useCallback(async () => {
    try {
      const raw = await commands.getSettings();
      const s = {
        downloadPath:
          raw.download_path || raw.downloadPath || settings.downloadPath,
        maxConcurrentDownloads: parseInt(
          raw.max_concurrent_downloads || "3",
          10,
        ),
        speedLimit: parseInt(raw.speed_limit || "0", 10),
        autoStartDownloads: raw.auto_start_download !== "false",
        theme: (raw.theme || "system") as "light" | "dark" | "system",
        language: raw.language || "en",
        notifications: raw.notifications !== "false",
        closeToTray: raw.close_to_tray === "true",
        autoLaunch: raw.auto_launch === "true",
        defaultQuality: raw.default_quality || "best",
        qualityPreset: (raw.quality_preset || "best") as
          | "best"
          | "4k"
          | "1080p"
          | "720p"
          | "audio",
        defaultFormat: raw.default_format || "mp4",
        embedThumbnail: raw.embed_thumbnail !== "false",
        embedMetadata: raw.embed_metadata !== "false",
        browserForCookies: raw.browser_cookies || "none",
        configPath: raw.config_file || "",
        ytdlpFlags: raw.ytdlp_flags || "",
        rssCheckInterval: parseInt(raw.rss_check_interval || "60", 10),
        rssNotifications: raw.rss_notifications !== "false",
        rssAutoDownload: raw.rss_auto_download === "true",
      };
      setSettings(s);
      setTheme(s.theme);
      if (s.language !== i18n.language) {
        i18n.changeLanguage(s.language);
      }
      setLoaded(true);
    } catch (err) {
      console.error("Failed to load settings:", err);
    }
  }, [setSettings, setLoaded, setTheme, i18n, settings.downloadPath]);

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    loadSettings();
  }, [loadSettings]);

  const saveSetting = useCallback(
    async (key: string, value: string) => {
      try {
        await commands.saveSetting(key, value);
        // Update local state
        const keyMap: Record<string, string> = {
          theme: "theme",
          language: "language",
          download_path: "downloadPath",
          max_concurrent_downloads: "maxConcurrentDownloads",
          speed_limit: "speedLimit",
          auto_start_download: "autoStartDownloads",
          notifications: "notifications",
          close_to_tray: "closeToTray",
          auto_launch: "autoLaunch",
          default_quality: "defaultQuality",
          quality_preset: "qualityPreset",
          default_format: "defaultFormat",
          embed_thumbnail: "embedThumbnail",
          embed_metadata: "embedMetadata",
          browser_cookies: "browserForCookies",
          config_file: "configPath",
          ytdlp_flags: "ytdlpFlags",
          rss_check_interval: "rssCheckInterval",
          rss_notifications: "rssNotifications",
          rss_auto_download: "rssAutoDownload",
        };
        const attrKey = keyMap[key];
        if (attrKey) {
          setSettings((prev: typeof settings) => ({
            ...prev,
            [attrKey]: value,
          }));
        }
      } catch (err) {
        toast.error(`Failed to save setting: ${err}`);
      }
    },
    [setSettings],
  );

  const selectDirectory = useCallback(async () => {
    try {
      const path = await commands.selectDirectory();
      if (path) {
        await saveSetting("download_path", path);
        return path;
      }
    } catch (err) {
      toast.error(`Failed to select directory: ${err}`);
    }
    return null;
  }, [saveSetting]);

  return { settings, loaded, loadSettings, saveSetting, selectDirectory };
}
