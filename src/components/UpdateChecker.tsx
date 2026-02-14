import { useEffect, useState } from "react";
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
        console.error("yt-dlp not installed:", err);
      }

      try {
        ytdlpLatest = await commands.getYtdlpLatestVersion();
      } catch (err) {
        console.error("Failed to check yt-dlp latest:", err);
      }

      const ytdlpNeedsUpdate =
        ytdlpCurrent && ytdlpLatest && ytdlpCurrent !== ytdlpLatest;

      // Check ffmpeg
      let ffmpegCurrent: string | null = null;
      let ffmpegHasUpdate = false;

      try {
        ffmpegCurrent = await commands.getFfmpegVersion();
      } catch (err) {
        console.error("ffmpeg not installed:", err);
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
          needsUpdate: !!ytdlpNeedsUpdate || !ytdlpCurrent,
        },
        ffmpeg: {
          current: ffmpegCurrent,
          hasUpdate: ffmpegHasUpdate || !ffmpegCurrent,
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
        toast.success("yt-dlp updated successfully");
      } catch (err) {
        console.error("yt-dlp update failed:", err);
        setUpdateProgress((prev) => ({ ...prev, ytdlp: "error" }));
        toast.error(`yt-dlp update failed: ${err}`);
      }
    }

    // Update ffmpeg
    if (updateInfo.ffmpeg.hasUpdate) {
      try {
        setUpdateProgress((prev) => ({ ...prev, ffmpeg: "updating" }));
        await commands.updateFfmpeg();
        setUpdateProgress((prev) => ({ ...prev, ffmpeg: "done" }));
        toast.success("ffmpeg updated successfully");
      } catch (err) {
        console.error("ffmpeg update failed:", err);
        setUpdateProgress((prev) => ({ ...prev, ffmpeg: "error" }));
        toast.error(`ffmpeg update failed: ${err}`);
      }
    }

    setUpdating(false);

    // Close dialog if all updates successful
    if (
      (!updateInfo.ytdlp.needsUpdate || updateProgress.ytdlp === "done") &&
      (!updateInfo.ffmpeg.hasUpdate || updateProgress.ffmpeg === "done")
    ) {
      setTimeout(() => setOpen(false), 1500);
    }
  };

  if (checking || !updateInfo) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Component Updates Available</DialogTitle>
          <DialogDescription>
            New versions of components are available. Update now to get the
            latest features and fixes.
          </DialogDescription>
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
                    : "Not installed"}
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
                  {updateInfo.ffmpeg.current || "Not installed"}
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
                Updating components...
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
            Skip
          </Button>
          <Button onClick={handleUpdate} disabled={updating}>
            {updating ? (
              <>
                <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                Updating...
              </>
            ) : (
              <>
                <Download className="w-4 h-4 mr-2" />
                Update Now
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
