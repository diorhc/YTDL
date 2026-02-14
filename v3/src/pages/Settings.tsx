import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import { FolderOpen, Moon, Sun, Monitor, Bug, Lightbulb } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { useTheme } from "next-themes";
import { commands } from "@/lib/tauri";
import { toast } from "sonner";

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { settings, saveSetting, selectDirectory } = useSettings();

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
      toast.error(`Failed to open feedback link: ${String(err)}`);
    }
  };

  return (
    <div className="flex flex-col h-full p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-bold">{t("settings.title")}</h1>
      </div>

      <Tabs defaultValue="general" className="flex-1 flex flex-col min-h-0">
        <TabsList className="grid w-full grid-cols-4 max-w-lg">
          <TabsTrigger value="general">{t("settings.general")}</TabsTrigger>
          <TabsTrigger value="downloads">{t("settings.downloads")}</TabsTrigger>
          <TabsTrigger value="rss">RSS</TabsTrigger>
          <TabsTrigger value="advanced">{t("settings.advanced")}</TabsTrigger>
        </TabsList>

        <ScrollArea className="flex-1 mt-4">
          {/* General Tab */}
          <TabsContent value="general" className="space-y-4">
            <Card>
              <CardContent className="p-0">
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

                {/* Close to tray */}
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

                <SettingItem
                  title="Feedback"
                  description="Report bugs and suggest new features"
                >
                  <div className="flex gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => void openFeedback("bug")}
                    >
                      <Bug className="w-4 h-4 mr-1.5" />
                      Report bug
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => void openFeedback("feature")}
                    >
                      <Lightbulb className="w-4 h-4 mr-1.5" />
                      Request feature
                    </Button>
                  </div>
                </SettingItem>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Downloads Tab */}
          <TabsContent value="downloads" className="space-y-4">
            <Card>
              <CardContent className="p-0">
                {/* Download path */}
                <SettingItem
                  title={t("settings.downloadPath")}
                  description={t("settings.downloadPathDesc")}
                >
                  <div className="flex gap-2 w-full max-w-md">
                    <Input
                      value={settings.downloadPath}
                      readOnly
                      className="flex-1"
                    />
                    <Button variant="outline" onClick={selectDirectory}>
                      <FolderOpen className="w-4 h-4 mr-1.5" />
                      {t("settings.selectPath")}
                    </Button>
                  </div>
                </SettingItem>

                <Separator />

                {/* Default quality */}
                <SettingItem
                  title={t("settings.defaultQuality")}
                  description={t("settings.defaultQualityDesc")}
                >
                  <div className="flex gap-2">
                    {["best", "1080p", "720p", "480p", "audio"].map((q) => (
                      <Button
                        key={q}
                        variant={
                          settings.defaultQuality === q ? "default" : "outline"
                        }
                        size="sm"
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
                  title="Quality preset"
                  description="Automatically select quality based on preset"
                >
                  <div className="flex gap-2 flex-wrap">
                    {[
                      { value: "best", label: "Best Available" },
                      { value: "4k", label: "4K when available" },
                      { value: "1080p", label: "Always 1080p" },
                      { value: "720p", label: "Always 720p" },
                      { value: "audio", label: "Audio only" },
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

                {/* Concurrent downloads */}
                <SettingItem
                  title="Concurrent downloads"
                  description="Maximum number of simultaneous downloads"
                >
                  <div className="flex gap-2">
                    {[1, 2, 3, 5, 10].map((n) => (
                      <Button
                        key={n}
                        variant={
                          settings.maxConcurrentDownloads === n
                            ? "default"
                            : "outline"
                        }
                        size="sm"
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

                {/* Download speed limit */}
                <SettingItem
                  title="Speed limit"
                  description="Limit download speed (0 = unlimited)"
                >
                  <div className="flex gap-2">
                    {[
                      { value: 0, label: "Unlimited" },
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
              </CardContent>
            </Card>
          </TabsContent>

          {/* RSS Tab */}
          <TabsContent value="rss" className="space-y-4">
            <Card>
              <CardContent className="p-0">
                {/* RSS Check Interval */}
                <SettingItem
                  title="Auto-check interval"
                  description="How often to automatically check RSS feeds for new videos"
                >
                  <div className="flex gap-2">
                    {[
                      { value: 0, label: "Off" },
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
                  title="New video notifications"
                  description="Show desktop notifications when new videos are found"
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
                  title="Auto-download new videos"
                  description="Automatically download videos from feeds with auto-download enabled"
                >
                  <Switch
                    checked={settings.rssAutoDownload ?? false}
                    onCheckedChange={(checked) =>
                      saveSetting("rss_auto_download", String(checked))
                    }
                  />
                </SettingItem>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Advanced Tab */}
          <TabsContent value="advanced" className="space-y-4">
            <Card>
              <CardContent className="p-0">
                {/* Browser for cookies */}
                <SettingItem
                  title={t("settings.browserForCookies")}
                  description={t("settings.browserForCookiesDesc")}
                >
                  <div className="flex gap-2">
                    {["none", "chrome", "firefox", "edge", "brave"].map((b) => (
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
                    ))}
                  </div>
                </SettingItem>

                <Separator />

                {/* yt-dlp flags */}
                <SettingItem
                  title={t("settings.ytdlpFlags")}
                  description={t("settings.ytdlpFlagsDesc")}
                >
                  <Input
                    placeholder={t("settings.ytdlpFlagsPlaceholder")}
                    className="max-w-md"
                    defaultValue={settings.ytdlpFlags}
                    onBlur={(e) => saveSetting("ytdlp_flags", e.target.value)}
                  />
                </SettingItem>

                <Separator />

                {/* Config file */}
                <SettingItem
                  title={t("settings.configFile")}
                  description={t("settings.configFileDesc")}
                >
                  <div className="flex gap-2 max-w-md">
                    <Input
                      value={settings.configPath}
                      readOnly
                      className="flex-1"
                    />
                    <Button
                      variant="secondary"
                      onClick={() => saveSetting("config_file", "")}
                    >
                      {t("settings.clearConfig")}
                    </Button>
                  </div>
                </SettingItem>
              </CardContent>
            </Card>
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
    <div className="flex items-center justify-between p-4">
      <div className="space-y-1 flex-1 mr-4">
        <p className="text-sm font-medium">{title}</p>
        {description && (
          <p className="text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}
