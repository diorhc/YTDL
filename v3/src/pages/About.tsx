import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Progress } from "@/components/ui/progress";
import {
  ExternalLink,
  Heart,
  Github,
  RefreshCw,
  Download,
  CheckCircle2,
  Loader2,
} from "lucide-react";
import { commands } from "@/lib/tauri";
import { toast } from "sonner";

const CREDITS = [
  { name: "Tauri", url: "https://tauri.app", roleKey: "framework" },
  {
    name: "yt-dlp",
    url: "https://github.com/yt-dlp/yt-dlp",
    roleKey: "downloadEngine",
  },
  { name: "FFmpeg", url: "https://ffmpeg.org", roleKey: "mediaProcessing" },
  {
    name: "whisper.cpp",
    url: "https://github.com/ggerganov/whisper.cpp",
    roleKey: "transcription",
  },
  { name: "React", url: "https://react.dev", roleKey: "uiLibrary" },
  { name: "shadcn/ui", url: "https://ui.shadcn.com", roleKey: "uiComponents" },
  { name: "Tailwind CSS", url: "https://tailwindcss.com", roleKey: "styling" },
];

type UpdateState =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "up-to-date"
  | "error";

export function AboutPage() {
  const { t } = useTranslation();
  const [version, setVersion] = useState("3.1.0");
  const [updateState, setUpdateState] = useState<UpdateState>("idle");
  const [updateVersion, setUpdateVersion] = useState<string>("");
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [platform, setPlatform] = useState("");

  useEffect(() => {
    commands
      .getAppVersion()
      .then(setVersion)
      .catch(() => {});
    commands
      .getPlatform()
      .then(setPlatform)
      .catch(() => {});
  }, []);

  const isDesktop = platform && platform !== "android" && platform !== "ios";

  const handleOpenUrl = async (url: string) => {
    try {
      await commands.openExternal(url);
    } catch {
      window.open(url, "_blank");
    }
  };

  const handleCheckUpdate = useCallback(async () => {
    // On mobile, just open the releases page
    if (!isDesktop) {
      handleOpenUrl("https://github.com/diorhc/YTDL/releases/latest");
      return;
    }

    setUpdateState("checking");
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();

      if (update) {
        setUpdateVersion(update.version);
        setUpdateState("available");
      } else {
        setUpdateState("up-to-date");
        toast.success(t("about.upToDate"));
      }
    } catch (err) {
      console.error("Update check failed:", err);
      setUpdateState("error");
      // Fallback: open releases page
      handleOpenUrl("https://github.com/diorhc/YTDL/releases/latest");
    }
  }, [isDesktop, t]);

  const handleDownloadAndInstall = useCallback(async () => {
    setUpdateState("downloading");
    setDownloadProgress(0);
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (!update) return;

      let totalLength = 0;
      let downloaded = 0;

      await update.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          totalLength = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (totalLength > 0) {
            setDownloadProgress(Math.round((downloaded / totalLength) * 100));
          }
        } else if (event.event === "Finished") {
          setDownloadProgress(100);
        }
      });

      setUpdateState("ready");
      toast.success(t("about.updateReady"));
    } catch (err) {
      console.error("Update download failed:", err);
      setUpdateState("error");
      toast.error(String(err));
    }
  }, [t]);

  const handleRestart = useCallback(async () => {
    try {
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch {
      toast.error("Failed to restart. Please restart the app manually.");
    }
  }, []);

  const renderUpdateSection = () => {
    switch (updateState) {
      case "checking":
        return (
          <div className="flex items-center gap-2">
            <Loader2 className="w-4 h-4 animate-spin text-primary" />
            <span className="text-xs text-muted-foreground">
              {t("about.checkUpdate")}...
            </span>
          </div>
        );

      case "available":
        return (
          <div className="space-y-3">
            <p className="text-sm font-medium text-primary">
              {t("about.updateAvailable")}: v{updateVersion}
            </p>
            <Button size="sm" onClick={handleDownloadAndInstall}>
              <Download className="w-4 h-4 mr-1.5" />
              {t("about.updateDownload")}
            </Button>
          </div>
        );

      case "downloading":
        return (
          <div className="space-y-2 w-full">
            <div className="flex items-center gap-2">
              <Loader2 className="w-4 h-4 animate-spin text-primary" />
              <span className="text-xs text-muted-foreground">
                {t("about.updateDownload")}... {downloadProgress}%
              </span>
            </div>
            <Progress value={downloadProgress} className="h-2" />
          </div>
        );

      case "ready":
        return (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <CheckCircle2 className="w-4 h-4 text-green-500" />
              <span className="text-sm font-medium text-green-500">
                {t("about.updateReady")}
              </span>
            </div>
            <Button size="sm" onClick={handleRestart}>
              <RefreshCw className="w-4 h-4 mr-1.5" />
              {t("about.updateReady")}
            </Button>
          </div>
        );

      case "up-to-date":
        return (
          <div className="flex items-center justify-between w-full">
            <div className="flex items-center gap-2">
              <CheckCircle2 className="w-4 h-4 text-green-500" />
              <span className="text-xs text-muted-foreground">
                {t("about.upToDate")}
              </span>
            </div>
            <Button variant="outline" size="sm" onClick={handleCheckUpdate}>
              <RefreshCw className="w-4 h-4 mr-1.5" />
              {t("about.checkUpdate")}
            </Button>
          </div>
        );

      default:
        return (
          <Button variant="outline" size="sm" onClick={handleCheckUpdate}>
            <RefreshCw className="w-4 h-4 mr-1.5" />
            {t("about.checkUpdate")}
          </Button>
        );
    }
  };

  return (
    <div className="flex flex-col h-full p-6 items-center">
      <div className="max-w-lg w-full space-y-6 py-8">
        {/* App identity */}
        <div className="flex flex-col items-center text-center space-y-3">
          <div>
            <h1 className="text-3xl font-bold tracking-tight">YTDL</h1>
            <p className="text-muted-foreground text-sm mt-1">
              {t("about.subtitle")}
            </p>
          </div>
        </div>

        {/* Update check */}
        <Card>
          <CardContent className="p-4 flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <p className="text-sm font-medium">
                {t("about.currentVersion")}: {version}
              </p>
            </div>
            {renderUpdateSection()}
          </CardContent>
        </Card>

        {/* Credits */}
        <Card>
          <CardContent className="p-0">
            <div className="p-4">
              <p className="text-sm font-medium mb-3">
                {t("about.credits.title")}
              </p>
              <div className="space-y-2">
                {CREDITS.map((c, i) => (
                  <div key={c.name}>
                    <div className="flex items-center justify-between py-1">
                      <div>
                        <p className="text-sm font-medium">{c.name}</p>
                        <p className="text-xs text-muted-foreground">
                          {t(`about.credits.${c.roleKey}`)}
                        </p>
                      </div>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => handleOpenUrl(c.url)}
                      >
                        <ExternalLink className="w-3.5 h-3.5" />
                      </Button>
                    </div>
                    {i < CREDITS.length - 1 && <Separator />}
                  </div>
                ))}
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Links */}
        <div className="flex justify-center gap-3">
          <Button
            variant="outline"
            size="sm"
            onClick={() => handleOpenUrl("https://github.com/diorhc/YTDL")}
          >
            <Github className="w-4 h-4 mr-1.5" />
            GitHub
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => handleOpenUrl("https://github.com/sponsors/diorhc")}
          >
            <Heart className="w-4 h-4 mr-1.5" />
            {t("about.sponsor")}
          </Button>
        </div>

        {/* Footer */}
        <p className="text-center text-xs text-muted-foreground">
          {t("about.license")}
        </p>
      </div>
    </div>
  );
}
