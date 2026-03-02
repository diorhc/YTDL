import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useAtomValue } from "jotai";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  CheckCircle2,
  XCircle,
  Download,
  Loader2,
  ArrowRight,
  RefreshCw,
  Copy,
  Check,
} from "lucide-react";
import { commands } from "@/lib/tauri";
import { platformAtom } from "@/store/atoms";
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

interface AndroidInfo {
  platform: string;
  termuxInstalled: boolean;
  termuxHasPermission: boolean;
  hasStoragePermission: boolean;
  nativeLibDir: string;
  bundledYtdlpWorks: boolean;
  bundledFfmpegWorks: boolean;
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard might be unavailable in some WebViews
    }
  };
  return (
    <button
      onClick={handleCopy}
      title="Copy to clipboard"
      className="flex-shrink-0 p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
    >
      {copied ? (
        <Check className="w-3.5 h-3.5 text-green-500" />
      ) : (
        <Copy className="w-3.5 h-3.5" />
      )}
    </button>
  );
}

function CommandBlock({ label, command }: { label: string; command: string }) {
  return (
    <div className="space-y-1">
      <p className="text-xs text-muted-foreground">{label}</p>
      <div className="flex items-start gap-1 rounded-md bg-muted px-3 py-2">
        <pre className="flex-1 text-xs whitespace-pre-wrap break-all leading-relaxed">
          {command}
        </pre>
        <CopyButton text={command} />
      </div>
    </div>
  );
}

export function SetupPage({ onComplete }: SetupPageProps) {
  const { t } = useTranslation();
  const [checking, setChecking] = useState(true);
  const platform = useAtomValue(platformAtom);
  const [androidInfo, setAndroidInfo] = useState<AndroidInfo | null>(null);
  const [probeResult, setProbeResult] = useState<{
    strategy: string;
    path: string;
    version?: string;
    works: boolean;
    error?: string;
  } | null>(null);
  const [components, setComponents] = useState<ComponentStatus[]>([
    {
      name: "yt-dlp",
      key: "ytdlp",
      description: "ytdlpDesc",
      installed: null,
      installing: false,
      progress: 0,
    },
    {
      name: "FFmpeg",
      key: "ffmpeg",
      description: "ffmpegDesc",
      installed: null,
      installing: false,
      progress: 0,
    },
  ]);

  const allReady = components.every((c) => c.installed === true);
  const anyMissing = components.some((c) => c.installed === false);
  const isAndroid = platform === "android";

  const termuxAllInOneCommand =
    "pkg update -y && pkg upgrade -y && pkg install -y python ffmpeg && pip install -U yt-dlp && echo 'allow-external-apps=true' >> ~/.termux/termux.properties && echo '=== Done! Restart Termux and re-open YTDL ==='";
  const [setupLaunching, setSetupLaunching] = useState(false);

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

  useEffect(() => {
    if (platform === "android") {
      commands
        .getAndroidInfo()
        .then(setAndroidInfo)
        .catch(() => setAndroidInfo(null));
    }
  }, [platform]);

  // Re-fetch Android info when the window regains focus (user may have
  // just returned from Settings after granting storage permission)
  useEffect(() => {
    if (platform !== "android") return;
    const onFocus = () => {
      commands
        .getAndroidInfo()
        .then(setAndroidInfo)
        .catch(() => {});
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [platform]);

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

  const runProbe = async () => {
    try {
      const result = await commands.probeYtdlp();
      setProbeResult(result);
    } catch (e) {
      setProbeResult({
        strategy: "none",
        path: "",
        works: false,
        error: String(e),
      });
    }
  };

  return (
    <div className="h-full overflow-y-auto px-4 py-6 md:p-8">
      <div className="mx-auto w-full max-w-md space-y-8 pb-6 md:pb-8">
        {/* Header */}
        <div className="text-center space-y-2">
          <h1 className="text-2xl font-bold">{t("setup.title")}</h1>
          <p className="text-sm text-muted-foreground">
            {isAndroid ? t("setup.androidDesc") : t("setup.desktopDesc")}
            {!isAndroid && anyMissing && " " + t("setup.clickInstall")}
          </p>
        </div>

        {isAndroid && (
          <div className="rounded-xl border border-border/50 bg-card p-4 space-y-3">
            <div>
              <p className="text-sm font-semibold">
                {t("setup.androidSetupTitle")}
              </p>
              <p className="text-xs text-muted-foreground mt-1">
                {t("setup.termuxExplanation")}
              </p>
            </div>

            {androidInfo && (
              <div className="rounded-md border border-border/40 bg-muted/40 p-3 space-y-1 text-xs">
                <p>
                  {t("setup.termuxInstalled")}{" "}
                  {androidInfo.termuxInstalled ? "✓" : "✗"}
                </p>
                <p>
                  {t("setup.termuxPermission")}{" "}
                  {androidInfo.termuxHasPermission ? (
                    <span className="text-green-500">✓</span>
                  ) : (
                    <span className="text-destructive">
                      ✗ {t("setup.required")}
                    </span>
                  )}
                </p>
                <p>
                  {t("setup.storagePermission")}{" "}
                  {androidInfo.hasStoragePermission ? (
                    <span className="text-green-500">✓</span>
                  ) : (
                    <span className="text-destructive">
                      ✗ {t("setup.required")}
                    </span>
                  )}
                </p>
              </div>
            )}

            {/* Storage permission request */}
            {androidInfo && !androidInfo.hasStoragePermission && (
              <div className="rounded-md border border-orange-500/30 bg-orange-500/10 p-3 space-y-2">
                <p className="text-xs font-medium text-orange-700 dark:text-orange-400">
                  {t("setup.storageRequired")}
                </p>
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full"
                  onClick={async () => {
                    try {
                      await commands.requestStoragePermission();
                    } catch {
                      // Fallback: will re-check when user returns
                    }
                  }}
                >
                  {t("setup.grantStorage")}
                </Button>
              </div>
            )}

            {/* Auto-install button */}
            {androidInfo?.termuxInstalled && (
              <Button
                variant="default"
                size="sm"
                className="w-full"
                disabled={setupLaunching}
                onClick={async () => {
                  setSetupLaunching(true);
                  try {
                    await commands.launchTermuxSetup();
                  } catch {
                    // Fallback: open Termux manually
                    commands.openTermux().catch(() => {});
                  } finally {
                    setSetupLaunching(false);
                  }
                }}
              >
                {setupLaunching ? (
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                ) : (
                  <Download className="w-4 h-4 mr-2" />
                )}
                {t("setup.autoInstallTermux")}
              </Button>
            )}

            <div className="border-t border-border/40 pt-3 mt-3">
              <CommandBlock
                label={t("setup.allInOne")}
                command={termuxAllInOneCommand}
              />
            </div>

            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                className="flex-1"
                onClick={() =>
                  commands.openExternal(
                    "https://f-droid.org/packages/com.termux/",
                  )
                }
              >
                {t("setup.openTermuxPage")}
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="flex-1"
                onClick={runProbe}
              >
                {t("setup.probeYtdlp")}
              </Button>
            </div>

            {probeResult && (
              <div className="rounded-md border border-border/40 bg-muted/40 p-3 text-xs space-y-1">
                <p>Strategy: {probeResult.strategy}</p>
                <p>Works: {probeResult.works ? "✓" : "✗"}</p>
                {probeResult.path && (
                  <p className="break-all">Path: {probeResult.path}</p>
                )}
                {probeResult.version && <p>Version: {probeResult.version}</p>}
                {probeResult.error && (
                  <p className="text-destructive break-all">
                    {probeResult.error}
                  </p>
                )}
              </div>
            )}
          </div>
        )}

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
                  {t(`setup.${comp.description}`)}
                </p>
                {comp.installing && (
                  <Progress value={comp.progress} className="h-1 mt-2" />
                )}
                {comp.error && (
                  <p className="text-xs text-destructive mt-1">{comp.error}</p>
                )}
              </div>

              {/* Action */}
              {!checking &&
                !comp.installed &&
                !comp.installing &&
                !isAndroid && (
                  <Button
                    size="sm"
                    variant="outline"
                    className="flex-shrink-0"
                    onClick={() => installComponent(comp.key)}
                  >
                    <Download className="w-3.5 h-3.5 mr-1" />
                    {t("setup.install")}
                  </Button>
                )}
            </div>
          ))}
        </div>

        {/* Actions */}
        <div className="flex flex-col gap-2">
          {anyMissing && !checking && !isAndroid && (
            <Button onClick={installAll} className="w-full rounded-lg">
              <Download className="w-4 h-4 mr-2" />
              {t("setup.installAllMissing")}
            </Button>
          )}

          {/* On Android: offer a "Termux is ready" continue even if status shows X */}
          {isAndroid && !checking && anyMissing && (
            <Button onClick={onComplete} className="w-full rounded-lg">
              <CheckCircle2 className="w-4 h-4 mr-2" />
              {t("setup.termuxContinue")}
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
                  {t("setup.continue")}
                  <ArrowRight className="w-4 h-4 ml-2" />
                </>
              ) : (
                <>
                  <RefreshCw className="w-4 h-4 mr-2" />
                  {t("setup.recheck")}
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
              {t("setup.skipForNow")}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
