import { useState, useEffect, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  CheckCircle2,
  XCircle,
  Download,
  Loader2,
  ArrowRight,
  RefreshCw,
} from "lucide-react";
import { commands } from "@/lib/tauri";
import { listen } from "@tauri-apps/api/event";

interface ComponentStatus {
  name: string;
  key: "ytdlp" | "ffmpeg";
  description: string;
  installed: boolean | null;
  installing: boolean;
  progress: number;
  error?: string;
}

interface SetupPageProps {
  onComplete: () => void;
}

export function SetupPage({ onComplete }: SetupPageProps) {
  const [checking, setChecking] = useState(true);
  const [components, setComponents] = useState<ComponentStatus[]>([
    {
      name: "yt-dlp",
      key: "ytdlp",
      description: "Video download engine — supports 1000+ sites",
      installed: null,
      installing: false,
      progress: 0,
    },
    {
      name: "FFmpeg",
      key: "ffmpeg",
      description: "Multimedia framework for format conversion & merging",
      installed: null,
      installing: false,
      progress: 0,
    },
  ]);

  const allReady = components.every((c) => c.installed === true);
  const anyMissing = components.some((c) => c.installed === false);

  const checkComponents = useCallback(async () => {
    setChecking(true);
    try {
      const [ytdlpOk, ffmpegOk] = await Promise.all([
        commands.checkYtdlp(),
        commands.checkFfmpeg(),
      ]);
      setComponents((prev) =>
        prev.map((c) => ({
          ...c,
          installed:
            c.key === "ytdlp"
              ? ytdlpOk
              : c.key === "ffmpeg"
                ? ffmpegOk
                : c.installed,
        })),
      );
    } catch (err) {
      console.error("Check failed:", err);
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    checkComponents();
  }, [checkComponents]);

  // Listen for install progress events
  useEffect(() => {
    const unlisten = listen<{
      tool: string;
      status: string;
      progress: number;
    }>("install-progress", (event) => {
      const { tool, status, progress } = event.payload;
      setComponents((prev) =>
        prev.map((c) => {
          if (
            (tool === "yt-dlp" && c.key === "ytdlp") ||
            (tool === "ffmpeg" && c.key === "ffmpeg")
          ) {
            return {
              ...c,
              progress,
              installing: status === "downloading",
              installed: status === "completed" ? true : c.installed,
            };
          }
          return c;
        }),
      );
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const installComponent = async (key: "ytdlp" | "ffmpeg") => {
    setComponents((prev) =>
      prev.map((c) =>
        c.key === key
          ? { ...c, installing: true, progress: 0, error: undefined }
          : c,
      ),
    );

    try {
      if (key === "ytdlp") {
        await commands.installYtdlp();
      } else {
        await commands.installFfmpeg();
      }
      setComponents((prev) =>
        prev.map((c) =>
          c.key === key
            ? { ...c, installing: false, installed: true, progress: 100 }
            : c,
        ),
      );
    } catch (err) {
      setComponents((prev) =>
        prev.map((c) =>
          c.key === key ? { ...c, installing: false, error: String(err) } : c,
        ),
      );
    }
  };

  const installAll = async () => {
    for (const comp of components) {
      if (!comp.installed) {
        await installComponent(comp.key);
      }
    }
  };

  return (
    <div className="flex flex-col items-center justify-center h-full p-8">
      <div className="w-full max-w-md space-y-8">
        {/* Header */}
        <div className="text-center space-y-2">
          <h1 className="text-2xl font-bold">Component Setup</h1>
          <p className="text-sm text-muted-foreground">
            YTDL requires these components to work properly.
            {anyMissing && " Click install to download them automatically."}
          </p>
        </div>

        {/* Components list */}
        <div className="space-y-3">
          {components.map((comp) => (
            <div
              key={comp.key}
              className="flex items-center gap-4 p-4 rounded-xl border border-border/50 bg-card"
            >
              {/* Status icon */}
              <div className="flex-shrink-0">
                {checking ? (
                  <Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
                ) : comp.installed ? (
                  <CheckCircle2 className="w-5 h-5 text-green-500" />
                ) : comp.installing ? (
                  <Loader2 className="w-5 h-5 animate-spin text-primary" />
                ) : (
                  <XCircle className="w-5 h-5 text-destructive" />
                )}
              </div>

              {/* Info */}
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium">{comp.name}</p>
                <p className="text-xs text-muted-foreground">
                  {comp.description}
                </p>
                {comp.installing && (
                  <Progress value={comp.progress} className="h-1 mt-2" />
                )}
                {comp.error && (
                  <p className="text-xs text-destructive mt-1">{comp.error}</p>
                )}
              </div>

              {/* Action */}
              {!checking && !comp.installed && !comp.installing && (
                <Button
                  size="sm"
                  variant="outline"
                  className="flex-shrink-0"
                  onClick={() => installComponent(comp.key)}
                >
                  <Download className="w-3.5 h-3.5 mr-1" />
                  Install
                </Button>
              )}
            </div>
          ))}
        </div>

        {/* Actions */}
        <div className="flex flex-col gap-2">
          {anyMissing && !checking && (
            <Button onClick={installAll} className="w-full rounded-lg">
              <Download className="w-4 h-4 mr-2" />
              Install All Missing
            </Button>
          )}

          {!checking && (
            <Button
              variant={allReady ? "default" : "outline"}
              onClick={allReady ? onComplete : checkComponents}
              className="w-full rounded-lg"
            >
              {allReady ? (
                <>
                  Continue
                  <ArrowRight className="w-4 h-4 ml-2" />
                </>
              ) : (
                <>
                  <RefreshCw className="w-4 h-4 mr-2" />
                  Re-check
                </>
              )}
            </Button>
          )}

          {!allReady && !checking && (
            <Button
              variant="ghost"
              onClick={onComplete}
              className="w-full text-muted-foreground"
            >
              Skip for now
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
