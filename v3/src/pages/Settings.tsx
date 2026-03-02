import { useTranslation } from "react-i18next";
import { useAtomValue } from "jotai";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import { FolderOpen, Moon, Sun, Monitor, Bug, Lightbulb } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { useTheme } from "next-themes";
import { commands } from "@/lib/tauri";
import { toast } from "sonner";
import { platformAtom } from "@/store/atoms";

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { settings, saveSetting, selectDirectory } = useSettings();
  const platform = useAtomValue(platformAtom);

  const handleThemeChange = (th: string) => {
    setTheme(th);
    saveSetting("theme", th);
  };

  const handleLanguageChange = (lang: string) => {
    i18n.changeLanguage(lang);
    saveSetting("language", lang);
  };

  const openFeedback = async (type: "bug" | "feature") => {
    const url =
      type === "bug"
        ? "https://github.com/diorhc/YouTube-Downloader/issues/new?labels=bug&title=%5BBug%5D%20"
        : "https://github.com/diorhc/YouTube-Downloader/issues/new?labels=enhancement&title=%5BFeature%5D%20";

    try {
      await commands.openExternal(url);
    } catch (err) {
      toast.error(t("settings.feedbackFailed", { error: String(err) }));
    }
  };

  return (
    <div className="flex flex-col h-full bg-background/50">
      <div className="px-4 sm:px-6 pt-6 pb-2 sm:pb-4">
        <h1 className="text-2xl sm:text-3xl font-bold tracking-tight">
          {t("settings.title")}
        </h1>
      </div>

      <Tabs
        defaultValue="general"
        className="flex-1 flex flex-col min-h-0 px-4 sm:px-6"
      >
        <TabsList className="inline-flex items-center bg-muted/50 p-1 rounded-full mb-6 shrink-0 h-10">
          <TabsTrigger
            value="general"
            className="rounded-full px-4 py-1.5 text-sm font-medium transition-all data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm"
          >
            {t("settings.general")}
          </TabsTrigger>
          <TabsTrigger
            value="downloads"
            className="rounded-full px-4 py-1.5 text-sm font-medium transition-all data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm"
          >
            {t("settings.downloads")}
          </TabsTrigger>
          <TabsTrigger
            value="rss"
            className="rounded-full px-4 py-1.5 text-sm font-medium transition-all data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm"
          >
            RSS
          </TabsTrigger>
          <TabsTrigger
            value="advanced"
            className="rounded-full px-4 py-1.5 text-sm font-medium transition-all data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm"
          >
            {t("settings.advanced")}
          </TabsTrigger>
        </TabsList>

        <ScrollArea className="flex-1 min-h-0 pb-6">
          {/* General Tab */}
          <TabsContent
            value="general"
            className="space-y-4 m-0 data-[state=active]:animate-in data-[state=active]:fade-in-50"
          >
            <div className="rounded-[24px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm overflow-hidden">
              <div className="flex flex-col divide-y divide-border/50 dark:divide-white/10">
                {/* Theme */}
                <SettingItem
                  title={t("settings.theme")}
                  description={t("settings.themeDesc")}
                >
                  <div className="flex gap-2">
                    {(["light", "dark", "system"] as const).map((th) => (
                      <Button
                        key={th}
                        variant={theme === th ? "default" : "outline"}
                        size="sm"
                        className="rounded-full"
                        onClick={() => handleThemeChange(th)}
                      >
                        {th === "light" && <Sun className="w-4 h-4 mr-1.5" />}
                        {th === "dark" && <Moon className="w-4 h-4 mr-1.5" />}
                        {th === "system" && (
                          <Monitor className="w-4 h-4 mr-1.5" />
                        )}
                        {t(`settings.${th}`)}
                      </Button>
                    ))}
                  </div>
                </SettingItem>

                <Separator />

                {/* Language */}
                <SettingItem
                  title={t("settings.language")}
                  description={t("settings.languageDesc")}
                >
                  <div className="flex gap-2">
                    {[
                      { code: "en", label: "English" },
                      { code: "ru", label: "Русский" },
                    ].map((lang) => (
                      <Button
                        key={lang.code}
                        variant={
                          i18n.language === lang.code ? "default" : "outline"
                        }
                        size="sm"
                        className="rounded-full"
                        onClick={() => handleLanguageChange(lang.code)}
                      >
                        {lang.label}
                      </Button>
                    ))}
                  </div>
                </SettingItem>

                <Separator />

                {/* Notifications */}
                <SettingItem
                  title={t("settings.notifications")}
                  description={t("settings.notificationsDesc")}
                >
                  <Switch
                    checked={settings.notifications}
                    onCheckedChange={(checked) =>
                      saveSetting("notifications", String(checked))
                    }
                  />
                </SettingItem>

                <Separator />

                {/* Close to tray — desktop only */}
                {platform !== "android" && (
                  <>
                    <SettingItem
                      title={t("settings.closeToTray")}
                      description={t("settings.closeToTrayDesc")}
                    >
                      <Switch
                        checked={settings.closeToTray}
                        onCheckedChange={(checked) =>
                          saveSetting("close_to_tray", String(checked))
                        }
                      />
                    </SettingItem>

                    <Separator />

                    {/* Auto launch */}
                    <SettingItem
                      title={t("settings.autoLaunch")}
                      description={t("settings.autoLaunchDesc")}
                    >
                      <Switch
                        checked={settings.autoLaunch}
                        onCheckedChange={(checked) =>
                          saveSetting("auto_launch", String(checked))
                        }
                      />
                    </SettingItem>

                    <Separator />
                  </>
                )}

                {/* Feedback */}
                <SettingItem
                  title={t("settings.feedback")}
                  description={t("settings.feedbackDesc")}
                >
                  <div className="flex gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      className="rounded-full h-9 bg-background/50 shadow-sm"
                      onClick={() => void openFeedback("bug")}
                    >
                      <Bug className="w-4 h-4 mr-1.5" />
                      {t("settings.reportBug")}
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      className="rounded-full h-9 bg-background/50 shadow-sm"
                      onClick={() => void openFeedback("feature")}
                    >
                      <Lightbulb className="w-4 h-4 mr-1.5" />
                      {t("settings.requestFeature")}
                    </Button>
                  </div>
                </SettingItem>
              </div>
            </div>
          </TabsContent>

          {/* Downloads Tab */}
          <TabsContent
            value="downloads"
            className="space-y-4 m-0 data-[state=active]:animate-in data-[state=active]:fade-in-50"
          >
            <div className="rounded-[24px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm overflow-hidden">
              <div className="flex flex-col divide-y divide-border/50 dark:divide-white/10">
                {/* Download path */}
                <SettingItem
                  title={t("settings.downloadPath")}
                  description={t("settings.downloadPathDesc")}
                >
                  <div className="flex flex-col gap-2 w-full sm:max-w-md">
                    <div className="flex flex-col sm:flex-row gap-2 items-end sm:items-center">
                      <Input
                        value={settings.downloadPath}
                        readOnly={platform !== "android"}
                        className="flex-1 w-full bg-background/50 rounded-full"
                        onChange={(e) => {
                          if (platform === "android") {
                            saveSetting("download_path", e.target.value);
                          }
                        }}
                      />
                      {platform !== "android" && (
                        <Button
                          variant="outline"
                          onClick={selectDirectory}
                          className="shrink-0 w-full sm:w-auto bg-background/50 rounded-full"
                        >
                          <FolderOpen className="w-4 h-4 mr-1.5" />
                          {t("settings.selectPath")}
                        </Button>
                      )}
                    </div>
                    {platform === "android" && (
                      <div className="flex gap-1.5 flex-wrap">
                        {[
                          "/sdcard/Download/YTDL",
                          "/sdcard/Movies/YTDL",
                          "/sdcard/Music/YTDL",
                        ].map((p) => (
                          <Button
                            key={p}
                            variant={
                              settings.downloadPath === p
                                ? "default"
                                : "outline"
                            }
                            size="sm"
                            className={`rounded-full h-7 text-xs shadow-sm ${settings.downloadPath !== p ? "bg-background/50" : ""}`}
                            onClick={() => saveSetting("download_path", p)}
                          >
                            {p.replace("/sdcard/", "")}
                          </Button>
                        ))}
                      </div>
                    )}
                  </div>
                </SettingItem>

                <Separator />

                {/* Default quality */}
                <SettingItem
                  title={t("settings.defaultQuality")}
                  description={t("settings.defaultQualityDesc")}
                >
                  <div className="flex gap-2 flex-wrap">
                    {["best", "1080p", "720p", "480p", "audio"].map((q) => (
                      <Button
                        key={q}
                        variant={
                          settings.defaultQuality === q ? "default" : "outline"
                        }
                        size="sm"
                        className={`rounded-full h-9 shadow-sm ${settings.defaultQuality !== q ? "bg-background/50" : ""}`}
                        onClick={() => saveSetting("default_quality", q)}
                      >
                        {q}
                      </Button>
                    ))}
                  </div>
                </SettingItem>

                <Separator />

                {/* Quick quality presets */}
                <SettingItem
                  title={t("settings.qualityPreset")}
                  description={t("settings.qualityPresetDesc")}
                >
                  <div className="flex gap-2 flex-wrap">
                    {[
                      { value: "best", label: t("settings.presetBest") },
                      { value: "4k", label: t("settings.preset4k") },
                      { value: "1080p", label: t("settings.preset1080p") },
                      { value: "720p", label: t("settings.preset720p") },
                      { value: "audio", label: t("settings.presetAudio") },
                    ].map((preset) => (
                      <Button
                        key={preset.value}
                        variant={
                          settings.qualityPreset === preset.value
                            ? "default"
                            : "outline"
                        }
                        size="sm"
                        onClick={() =>
                          saveSetting("quality_preset", preset.value)
                        }
                      >
                        {preset.label}
                      </Button>
                    ))}
                  </div>
                </SettingItem>

                <Separator />

                {/* Concurrent downloads & speed limit — desktop only (Termux manages its own) */}
                {platform !== "android" && (
                  <>
                    <SettingItem
                      title={t("settings.concurrentDownloads")}
                      description={t("settings.concurrentDownloadsDesc")}
                    >
                      <div className="flex gap-2 flex-wrap">
                        {[1, 2, 3, 5, 10].map((n) => (
                          <Button
                            key={n}
                            variant={
                              settings.maxConcurrentDownloads === n
                                ? "default"
                                : "outline"
                            }
                            size="sm"
                            className={`rounded-full h-9 shadow-sm ${settings.maxConcurrentDownloads !== n ? "bg-background/50" : ""}`}
                            onClick={() =>
                              saveSetting("max_concurrent_downloads", String(n))
                            }
                          >
                            {n}
                          </Button>
                        ))}
                      </div>
                    </SettingItem>

                    <Separator />

                    <SettingItem
                      title={t("settings.speedLimit")}
                      description={t("settings.speedLimitDesc")}
                    >
                      <div className="flex gap-2 flex-wrap">
                        {[
                          { value: 0, label: t("settings.unlimited") },
                          { value: 1, label: "1 MB/s" },
                          { value: 2, label: "2 MB/s" },
                          { value: 5, label: "5 MB/s" },
                          { value: 10, label: "10 MB/s" },
                        ].map((opt) => (
                          <Button
                            key={opt.value}
                            variant={
                              settings.speedLimit === opt.value
                                ? "default"
                                : "outline"
                            }
                            size="sm"
                            className={`rounded-full h-9 shadow-sm ${settings.speedLimit !== opt.value ? "bg-background/50" : ""}`}
                            onClick={() =>
                              saveSetting("speed_limit", String(opt.value))
                            }
                          >
                            {opt.label}
                          </Button>
                        ))}
                      </div>
                    </SettingItem>

                    <Separator />
                  </>
                )}

                {/* Auto start */}
                <SettingItem
                  title={t("settings.autoStart")}
                  description={t("settings.autoStartDesc")}
                >
                  <Switch
                    checked={settings.autoStartDownloads}
                    onCheckedChange={(checked) =>
                      saveSetting("auto_start_download", String(checked))
                    }
                  />
                </SettingItem>

                <Separator />

                {/* Embed thumbnail */}
                <SettingItem
                  title={t("settings.embedThumbnail")}
                  description={t("settings.embedThumbnailDesc")}
                >
                  <Switch
                    checked={settings.embedThumbnail}
                    onCheckedChange={(checked) =>
                      saveSetting("embed_thumbnail", String(checked))
                    }
                  />
                </SettingItem>

                <Separator />

                {/* Embed metadata */}
                <SettingItem
                  title={t("settings.embedMetadata")}
                  description={t("settings.embedMetadataDesc")}
                >
                  <Switch
                    checked={settings.embedMetadata}
                    onCheckedChange={(checked) =>
                      saveSetting("embed_metadata", String(checked))
                    }
                  />
                </SettingItem>
              </div>
            </div>
          </TabsContent>

          {/* RSS Tab */}
          <TabsContent
            value="rss"
            className="space-y-4 m-0 data-[state=active]:animate-in data-[state=active]:fade-in-50"
          >
            <div className="rounded-[24px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm overflow-hidden">
              <div className="flex flex-col divide-y divide-border/50 dark:divide-white/10">
                {/* RSS Check Interval */}
                <SettingItem
                  title={t("settings.rssCheckInterval")}
                  description={t("settings.rssCheckIntervalDesc")}
                >
                  <div className="flex gap-2 flex-wrap">
                    {[
                      { value: 0, label: t("settings.off") },
                      { value: 15, label: "15 min" },
                      { value: 30, label: "30 min" },
                      { value: 60, label: "1 hour" },
                      { value: 120, label: "2 hours" },
                      { value: 360, label: "6 hours" },
                    ].map((opt) => (
                      <Button
                        key={opt.value}
                        variant={
                          settings.rssCheckInterval === opt.value
                            ? "default"
                            : "outline"
                        }
                        size="sm"
                        className={`rounded-full h-9 shadow-sm ${settings.rssCheckInterval !== opt.value ? "bg-background/50" : ""}`}
                        onClick={() =>
                          saveSetting("rss_check_interval", String(opt.value))
                        }
                      >
                        {opt.label}
                      </Button>
                    ))}
                  </div>
                </SettingItem>

                <Separator />

                {/* RSS Notifications */}
                <SettingItem
                  title={t("settings.rssNewVideoNotifications")}
                  description={t("settings.rssNewVideoNotificationsDesc")}
                >
                  <Switch
                    checked={settings.rssNotifications ?? true}
                    onCheckedChange={(checked) =>
                      saveSetting("rss_notifications", String(checked))
                    }
                  />
                </SettingItem>

                <Separator />

                {/* Auto-download */}
                <SettingItem
                  title={t("settings.rssAutoDownload")}
                  description={t("settings.rssAutoDownloadDesc")}
                >
                  <Switch
                    checked={settings.rssAutoDownload ?? false}
                    onCheckedChange={(checked) =>
                      saveSetting("rss_auto_download", String(checked))
                    }
                  />
                </SettingItem>
              </div>
            </div>
          </TabsContent>

          {/* Advanced Tab */}
          <TabsContent
            value="advanced"
            className="space-y-4 m-0 data-[state=active]:animate-in data-[state=active]:fade-in-50"
          >
            <div className="rounded-[24px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm overflow-hidden">
              <div className="flex flex-col divide-y divide-border/50 dark:divide-white/10">
                {/* Browser for cookies — desktop only (on Android, Termux uses its own cookies) */}
                {platform !== "android" && (
                  <>
                    <SettingItem
                      title={t("settings.browserForCookies")}
                      description={t("settings.browserForCookiesDesc")}
                    >
                      <div className="flex gap-2">
                        {["none", "chrome", "firefox", "edge", "brave"].map(
                          (b) => (
                            <Button
                              key={b}
                              variant={
                                settings.browserForCookies === b
                                  ? "default"
                                  : "outline"
                              }
                              size="sm"
                              onClick={() => saveSetting("browser_cookies", b)}
                            >
                              {b === "none" ? t("settings.none") : b}
                            </Button>
                          ),
                        )}
                      </div>
                    </SettingItem>

                    <Separator />
                  </>
                )}

                {/* yt-dlp flags */}
                <SettingItem
                  title={t("settings.ytdlpFlags")}
                  description={t("settings.ytdlpFlagsDesc")}
                >
                  <Input
                    placeholder={t("settings.ytdlpFlagsPlaceholder")}
                    className="max-w-md rounded-full"
                    defaultValue={settings.ytdlpFlags}
                    onBlur={(e) => saveSetting("ytdlp_flags", e.target.value)}
                  />
                </SettingItem>

                <Separator />

                {/* Config file — desktop only */}
                {platform !== "android" && (
                  <SettingItem
                    title={t("settings.configFile")}
                    description={t("settings.configFileDesc")}
                  >
                    <div className="flex gap-2 flex-col sm:flex-row w-full sm:max-w-md items-end sm:items-center">
                      <Input
                        value={settings.configPath}
                        readOnly
                        className="flex-1 w-full bg-background/50"
                      />
                      <Button
                        variant="secondary"
                        onClick={() => saveSetting("config_file", "")}
                        className="w-full sm:w-auto"
                      >
                        {t("settings.clearConfig")}
                      </Button>
                    </div>
                  </SettingItem>
                )}
              </div>
            </div>
          </TabsContent>
        </ScrollArea>
      </Tabs>
    </div>
  );
}

function SettingItem({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col sm:flex-row sm:items-center justify-between p-4 sm:p-5 gap-4">
      <div className="space-y-1 sm:flex-1 sm:pr-8">
        <p className="text-sm font-semibold">{title}</p>
        {description && (
          <p className="text-xs text-muted-foreground leading-relaxed">
            {description}
          </p>
        )}
      </div>
      <div className="flex-shrink-0 w-full sm:w-auto sm:text-right">
        {children}
      </div>
    </div>
  );
}
