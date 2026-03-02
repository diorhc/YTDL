import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { useAtomValue } from "jotai";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Download,
  Rss,
  Mic,
  ArrowRight,
  TrendingDown,
  CheckCircle2,
  AlertCircle,
  Clock,
  HardDrive,
  Activity,
} from "lucide-react";
import { downloadsAtom, feedsAtom, platformAtom } from "@/store/atoms";
import { useDownloads } from "@/hooks/useDownloads";
import { useRss } from "@/hooks/useRss";
import type { DownloadItem } from "@/lib/tauri";
import { formatBytes } from "@/lib/utils";

export function DashboardPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const downloads = useAtomValue(downloadsAtom);
  const feeds = useAtomValue(feedsAtom);
  const platform = useAtomValue(platformAtom);
  useDownloads(); // Ensures event listeners are registered and data is loaded
  useRss();

  const stats = useMemo(() => {
    const active = downloads.filter(
      (d) => d.status === "downloading" || d.status === "queued",
    ).length;
    const completed = downloads.filter((d) => d.status === "completed").length;
    const failed = downloads.filter((d) => d.status === "error").length;
    const totalSize = downloads
      .filter((d) => d.status === "completed" && d.fileSize)
      .reduce((sum, d) => sum + (d.fileSize || 0), 0);

    return { total: downloads.length, active, completed, failed, totalSize };
  }, [downloads]);

  const recentDownloads = useMemo(
    () =>
      [...downloads]
        .sort(
          (a, b) =>
            new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime(),
        )
        .slice(0, 5),
    [downloads],
  );

  const activeDownloads = useMemo(
    () =>
      downloads.filter(
        (d) => d.status === "downloading" || d.status === "queued",
      ),
    [downloads],
  );

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="px-4 sm:px-6 pt-6 pb-4">
        <h1 className="text-2xl text-muted-foreground mt-0.5 header-subtitle">
          {t("app.description")}
        </h1>
      </div>

      <ScrollArea className="flex-1 px-4 sm:px-6">
        <div className="space-y-6 pb-6">
          {/* Stats Grid */}
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 lg:gap-4">
            <StatCard
              icon={<Download className="w-5 h-5" />}
              label={t("dashboard.totalDownloads")}
              value={String(stats.total)}
              color="text-primary dark:text-primary-foreground"
              bgColor="bg-primary/10 dark:bg-primary/20"
              gradient="from-primary/10 to-transparent"
            />
            <StatCard
              icon={<Activity className="w-5 h-5" />}
              label={t("dashboard.active")}
              value={String(stats.active)}
              color="text-blue-500 dark:text-blue-400"
              bgColor="bg-blue-500/10 dark:bg-blue-500/20"
              gradient="from-blue-500/10 to-transparent"
            />
            <StatCard
              icon={<CheckCircle2 className="w-5 h-5" />}
              label={t("download.completed")}
              value={String(stats.completed)}
              color="text-emerald-500 dark:text-emerald-400"
              bgColor="bg-emerald-500/10 dark:bg-emerald-500/20"
              gradient="from-emerald-500/10 to-transparent"
            />
            <StatCard
              icon={<HardDrive className="w-5 h-5" />}
              label={t("dashboard.storageUsed")}
              value={formatBytes(stats.totalSize)}
              color="text-amber-500 dark:text-amber-400"
              bgColor="bg-amber-500/10 dark:bg-amber-500/20"
              gradient="from-amber-500/10 to-transparent"
            />
          </div>

          {/* Quick Actions */}
          <div
            className={
              platform === "android"
                ? "flex flex-row gap-3 justify-center"
                : "flex flex-col sm:grid sm:grid-cols-3 gap-3"
            }
          >
            <QuickAction
              icon={<Download className="w-6 h-6" />}
              title={t("nav.download")}
              description={t("dashboard.downloadDesc")}
              onClick={() => navigate("/download")}
              gradient="from-primary/20 to-primary/5"
              iconColor="text-primary"
              isCompact={platform === "android"}
            />
            <QuickAction
              icon={<Rss className="w-6 h-6" />}
              title={t("nav.rss")}
              description={t("dashboard.subscriptions", {
                count: feeds.length,
              })}
              onClick={() => navigate("/rss")}
              gradient="from-purple-500/20 to-purple-500/5"
              iconColor="text-purple-500"
              isCompact={platform === "android"}
            />
            <QuickAction
              icon={<Mic className="w-6 h-6" />}
              title={t("nav.transcribe")}
              description={t("dashboard.transcribeDesc")}
              onClick={() => navigate("/transcribe")}
              gradient="from-emerald-500/20 to-emerald-500/5"
              iconColor="text-emerald-500"
              isCompact={platform === "android"}
            />
          </div>

          {/* Active Downloads */}
          {activeDownloads.length > 0 && (
            <Card>
              <CardContent className="p-4">
                <div className="flex items-center justify-between mb-3">
                  <h2 className="text-sm font-semibold flex items-center gap-2">
                    <Activity className="w-4 h-4 text-blue-500" />
                    {t("dashboard.activeDownloads")}
                  </h2>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-xs"
                    onClick={() => navigate("/download")}
                  >
                    {t("dashboard.viewAll")}
                    <ArrowRight className="w-3 h-3 ml-1" />
                  </Button>
                </div>
                <div className="space-y-3">
                  {activeDownloads.slice(0, 3).map((dl) => (
                    <ActiveDownloadRow key={dl.id} download={dl} />
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {/* Recent Downloads */}
          {recentDownloads.length > 0 ? (
            <Card>
              <CardContent className="p-4">
                <div className="flex items-center justify-between mb-3">
                  <h2 className="text-sm font-semibold flex items-center gap-2">
                    <Clock className="w-4 h-4 text-muted-foreground" />
                    {t("dashboard.recentDownloads")}
                  </h2>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-xs"
                    onClick={() => navigate("/download")}
                  >
                    {t("dashboard.viewAll")}
                    <ArrowRight className="w-3 h-3 ml-1" />
                  </Button>
                </div>
                <div className="space-y-2">
                  {recentDownloads.map((dl) => (
                    <RecentDownloadRow key={dl.id} download={dl} />
                  ))}
                </div>
              </CardContent>
            </Card>
          ) : (
            <Card>
              <CardContent className="flex flex-col items-center justify-center py-12 text-center">
                <TrendingDown className="w-10 h-10 text-muted-foreground/20 mb-3" />
                <p className="text-sm font-medium text-muted-foreground">
                  {t("download.noDownloads")}
                </p>
                <p className="text-xs text-muted-foreground/60 mt-1 mb-4">
                  {t("download.noDownloadsDesc")}
                </p>
                <Button
                  size="sm"
                  className="rounded-lg"
                  onClick={() => navigate("/download")}
                >
                  <Download className="w-3.5 h-3.5 mr-1.5" />
                  {t("dashboard.startDownloading")}
                </Button>
              </CardContent>
            </Card>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

/* ─── Sub Components ──────────────────────────────────── */

function StatCard({
  icon,
  label,
  value,
  color,
  bgColor,
  gradient,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  color: string;
  bgColor: string;
  gradient?: string;
}) {
  return (
    <div className="relative overflow-hidden rounded-[20px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 p-4 shadow-sm">
      {gradient && (
        <div
          className={`absolute inset-0 bg-gradient-to-br ${gradient} opacity-50 pointer-events-none`}
        />
      )}
      <div className="relative z-10 flex flex-col gap-3">
        <div
          className={`w-10 h-10 rounded-xl ${bgColor} flex items-center justify-center ${color} shadow-sm`}
        >
          {icon}
        </div>
        <div>
          <p className="text-xl sm:text-2xl font-bold tracking-tight">
            {value}
          </p>
          <p className="text-xs text-muted-foreground mt-0.5 font-medium">
            {label}
          </p>
        </div>
      </div>
    </div>
  );
}

function QuickAction({
  icon,
  title,
  description,
  onClick,
  gradient,
  iconColor,
  isCompact = false,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  onClick: () => void;
  gradient: string;
  iconColor: string;
  isCompact?: boolean;
}) {
  if (isCompact) {
    // Compact mobile version - icon only
    return (
      <button
        onClick={onClick}
        className={`relative overflow-hidden group flex items-center justify-center w-16 h-16 rounded-full bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 transition-all active:scale-95 shadow-sm hover:shadow-md ${iconColor}`}
      >
        <div
          className={`absolute inset-0 bg-gradient-to-r ${gradient} opacity-30 group-hover:opacity-50 transition-opacity`}
        />
        <div className="relative z-10">{icon}</div>
      </button>
    );
  }

  return (
    <button
      onClick={onClick}
      className="relative overflow-hidden group flex items-center gap-4 p-4 rounded-[20px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 text-left transition-all active:scale-[0.98] shadow-sm hover:shadow-md"
    >
      <div
        className={`absolute inset-0 bg-gradient-to-r ${gradient} opacity-30 group-hover:opacity-50 transition-opacity`}
      />
      <div
        className={`relative z-10 w-12 h-12 rounded-2xl bg-background/50 backdrop-blur-sm border border-white/10 flex items-center justify-center ${iconColor} shadow-sm`}
      >
        {icon}
      </div>
      <div className="relative z-10 flex-1">
        <p className="text-base font-semibold">{title}</p>
        <p className="text-xs text-muted-foreground leading-snug mt-0.5">
          {description}
        </p>
      </div>
    </button>
  );
}

function ActiveDownloadRow({ download }: { download: DownloadItem }) {
  return (
    <div className="flex items-center gap-4 p-3 rounded-[16px] bg-background/50 backdrop-blur-md border border-border/50 dark:border-white/10 hover:bg-accent/20 transition-colors">
      <div className="relative w-16 h-12 rounded-xl overflow-hidden flex-shrink-0 bg-muted/50 border border-border/50 dark:border-white/10 shadow-sm">
        {download.thumbnail ? (
          <img
            src={download.thumbnail}
            alt=""
            className="w-full h-full object-cover"
          />
        ) : (
          <div className="flex items-center justify-center w-full h-full">
            <Activity className="w-5 h-5 text-muted-foreground/50" />
          </div>
        )}
        <div className="absolute inset-0 bg-black/20" />
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-sm font-semibold truncate leading-tight">
          {download.title}
        </p>
        <div className="flex items-center gap-3 mt-1.5">
          <div className="flex-1 h-1.5 bg-muted/50 rounded-full overflow-hidden">
            <div
              className="h-full bg-blue-500 rounded-full transition-all duration-300"
              style={{ width: `${download.progress}%` }}
            />
          </div>
          <span className="text-[10px] font-medium text-muted-foreground w-8 text-right flex-shrink-0">
            {Math.round(download.progress)}%
          </span>
        </div>
      </div>
    </div>
  );
}

function RecentDownloadRow({ download }: { download: DownloadItem }) {
  const formatText = (() => {
    const label = download.formatLabel?.trim();
    if (label) return label;
    const fileName = download.filePath?.split(/[\\/]/).pop() ?? "";
    const ext = fileName.includes(".") ? fileName.split(".").pop() : "";
    return ext ? ext.toUpperCase() : "—";
  })();

  const statusIcon = () => {
    switch (download.status) {
      case "completed":
        return <CheckCircle2 className="w-4 h-4 text-emerald-500" />;
      case "error":
        return <AlertCircle className="w-4 h-4 text-destructive" />;
      case "downloading":
      case "queued":
        return <Activity className="w-4 h-4 text-blue-500 animate-pulse" />;
      default:
        return <Clock className="w-4 h-4 text-muted-foreground" />;
    }
  };

  return (
    <div className="flex items-center gap-4 p-3 rounded-[16px] hover:bg-muted/30 transition-colors border border-transparent hover:border-border/50 dark:hover:border-white/10">
      <div className="relative w-14 h-10 rounded-xl overflow-hidden flex-shrink-0 bg-muted/50 border border-border/50 dark:border-white/10 shadow-sm">
        {download.thumbnail ? (
          <img
            src={download.thumbnail}
            alt=""
            className="w-full h-full object-cover"
          />
        ) : (
          <div className="flex items-center justify-center w-full h-full">
            <Clock className="w-4 h-4 text-muted-foreground/50" />
          </div>
        )}
      </div>
      <div className="flex-1 min-w-0 flex flex-col justify-center">
        <p className="text-sm font-medium truncate leading-snug">
          {download.title}
        </p>
        <p className="text-[11px] text-muted-foreground mt-0.5 font-medium">
          {formatText}
          {download.fileSize ? ` · ${formatBytes(download.fileSize)}` : ""}
        </p>
      </div>
      <div className="flex-shrink-0 w-8 flex items-center justify-end">
        {statusIcon()}
      </div>
    </div>
  );
}

/* ─── Helpers ──────────────────────────────────────────── */

export default DashboardPage;
