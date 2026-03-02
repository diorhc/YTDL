import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/lib/tauri";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Loader2, Download, CheckCircle2, AlertCircle } from "lucide-react";
import { toast } from "sonner";

interface UpdateCheckResult {
  ytdlp: {
    current: string | null;
    latest: string | null;
    needsUpdate: boolean;
  };
  ffmpeg: {
    current: string | null;
    hasUpdate: boolean;
  };
}

export function UpdateChecker() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [checking, setChecking] = useState(true);
  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null);
  const [updating, setUpdating] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<{
    ytdlp: "idle" | "updating" | "done" | "error";
    ffmpeg: "idle" | "updating" | "done" | "error";
  }>({ ytdlp: "idle", ffmpeg: "idle" });

  useEffect(() => {
    checkForUpdates();
  }, []);

  const checkForUpdates = async () => {
    setChecking(true);
    try {
      // Check yt-dlp
      let ytdlpCurrent: string | null = null;
      let ytdlpLatest: string | null = null;

      try {
        ytdlpCurrent = await commands.getYtdlpVersion();
      } catch (err) {
        console.error("yt-dlp version check failed:", err);
      }

      try {
        ytdlpLatest = await commands.getYtdlpLatestVersion();
      } catch (err) {
        console.error("Failed to check yt-dlp latest:", err);
      }

      // Only show update if both versions are available and they differ
      // Don't treat "version check failed" as "needs update" — it may be a permission issue
      const ytdlpNeedsUpdate =
        ytdlpCurrent && ytdlpLatest && ytdlpCurrent !== ytdlpLatest;

      // Check ffmpeg
      let ffmpegCurrent: string | null = null;
      let ffmpegHasUpdate = false;

      try {
        ffmpegCurrent = await commands.getFfmpegVersion();
      } catch (err) {
        console.error("ffmpeg version check failed:", err);
      }

      try {
        ffmpegHasUpdate = await commands.checkFfmpegUpdate();
      } catch (err) {
        console.error("Failed to check ffmpeg update:", err);
      }

      const result: UpdateCheckResult = {
        ytdlp: {
          current: ytdlpCurrent,
          latest: ytdlpLatest,
          // Only show update when we have both versions and they differ.
          // If version couldn't be retrieved (permission denied, not installed),
          // the Setup page handles that — not the update checker.
          needsUpdate: !!ytdlpNeedsUpdate,
        },
        ffmpeg: {
          current: ffmpegCurrent,
          // Only show ffmpeg update if the update check API explicitly says so
          hasUpdate: ffmpegHasUpdate,
        },
      };

      setUpdateInfo(result);

      // Show dialog if any updates are needed
      if (result.ytdlp.needsUpdate || result.ffmpeg.hasUpdate) {
        setOpen(true);
      }
    } catch (err) {
      console.error("Update check failed:", err);
    } finally {
      setChecking(false);
    }
  };

  const handleUpdate = async () => {
    if (!updateInfo) return;
    setUpdating(true);

    // Update yt-dlp
    if (updateInfo.ytdlp.needsUpdate) {
      try {
        setUpdateProgress((prev) => ({ ...prev, ytdlp: "updating" }));
        await commands.updateYtdlp();
        setUpdateProgress((prev) => ({ ...prev, ytdlp: "done" }));
        toast.success(t("update.ytdlpSuccess"));
      } catch (err) {
        console.error("yt-dlp update failed:", err);
        setUpdateProgress((prev) => ({ ...prev, ytdlp: "error" }));
        toast.error(t("update.ytdlpFailed", { error: String(err) }));
      }
    }

    // Update ffmpeg
    if (updateInfo.ffmpeg.hasUpdate) {
      try {
        setUpdateProgress((prev) => ({ ...prev, ffmpeg: "updating" }));
        await commands.updateFfmpeg();
        setUpdateProgress((prev) => ({ ...prev, ffmpeg: "done" }));
        toast.success(t("update.ffmpegSuccess"));
      } catch (err) {
        console.error("ffmpeg update failed:", err);
        setUpdateProgress((prev) => ({ ...prev, ffmpeg: "error" }));
        toast.error(t("update.ffmpegFailed", { error: String(err) }));
      }
    }

    setUpdating(false);

    // Close dialog if all updates were successful.
    // We read from the `current` state via the functional form since
    // React state updates are async and the local `updateProgress`
    // variable is stale inside this handler.
    setUpdateProgress((cur) => {
      const allDone =
        (!updateInfo.ytdlp.needsUpdate || cur.ytdlp === "done") &&
        (!updateInfo.ffmpeg.hasUpdate || cur.ffmpeg === "done");
      if (allDone) {
        setTimeout(() => setOpen(false), 1500);
      }
      return cur;
    });
  };

  if (checking || !updateInfo) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("update.title")}</DialogTitle>
          <DialogDescription>{t("update.description")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* yt-dlp update */}
          {updateInfo.ytdlp.needsUpdate && (
            <div className="flex items-center justify-between p-3 rounded-lg border">
              <div className="flex-1">
                <div className="font-medium">yt-dlp</div>
                <div className="text-sm text-muted-foreground">
                  {updateInfo.ytdlp.current
                    ? `${updateInfo.ytdlp.current} → ${updateInfo.ytdlp.latest}`
                    : t("update.notInstalled")}
                </div>
              </div>
              <div>
                {updateProgress.ytdlp === "idle" && (
                  <Download className="w-5 h-5 text-muted-foreground" />
                )}
                {updateProgress.ytdlp === "updating" && (
                  <Loader2 className="w-5 h-5 animate-spin text-primary" />
                )}
                {updateProgress.ytdlp === "done" && (
                  <CheckCircle2 className="w-5 h-5 text-green-500" />
                )}
                {updateProgress.ytdlp === "error" && (
                  <AlertCircle className="w-5 h-5 text-destructive" />
                )}
              </div>
            </div>
          )}

          {/* ffmpeg update */}
          {updateInfo.ffmpeg.hasUpdate && (
            <div className="flex items-center justify-between p-3 rounded-lg border">
              <div className="flex-1">
                <div className="font-medium">ffmpeg</div>
                <div className="text-sm text-muted-foreground">
                  {updateInfo.ffmpeg.current || t("update.notInstalled")}
                </div>
              </div>
              <div>
                {updateProgress.ffmpeg === "idle" && (
                  <Download className="w-5 h-5 text-muted-foreground" />
                )}
                {updateProgress.ffmpeg === "updating" && (
                  <Loader2 className="w-5 h-5 animate-spin text-primary" />
                )}
                {updateProgress.ffmpeg === "done" && (
                  <CheckCircle2 className="w-5 h-5 text-green-500" />
                )}
                {updateProgress.ffmpeg === "error" && (
                  <AlertCircle className="w-5 h-5 text-destructive" />
                )}
              </div>
            </div>
          )}

          {updating && (
            <div className="space-y-2">
              <Progress value={33} className="h-2" />
              <p className="text-xs text-center text-muted-foreground">
                {t("update.updatingComponents")}
              </p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => setOpen(false)}
            disabled={updating}
          >
            {t("update.skip")}
          </Button>
          <Button onClick={handleUpdate} disabled={updating}>
            {updating ? (
              <>
                <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                {t("update.updating")}
              </>
            ) : (
              <>
                <Download className="w-4 h-4 mr-2" />
                {t("update.updateNow")}
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
