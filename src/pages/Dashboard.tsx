import { useMemo, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { useAtom } from "jotai";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
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
import { downloadsAtom, feedsAtom } from "@/store/atoms";
import { useDownloads } from "@/hooks/useDownloads";
import { useRss } from "@/hooks/useRss";
import type { DownloadItem } from "@/lib/tauri";

export function DashboardPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [downloads] = useAtom(downloadsAtom);
  const [feeds] = useAtom(feedsAtom);
  const { loadDownloads } = useDownloads();
  const { loadFeeds } = useRss();

  useEffect(() => {
    loadDownloads();
    loadFeeds();
  }, [loadDownloads, loadFeeds]);

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
      <div className="px-6 pt-6 pb-4">
        <h1 className="text-2xl text-muted-foreground mt-0.5 header-subtitle">
          {t("app.description")}
        </h1>
      </div>

      <ScrollArea className="flex-1 px-6">
        <div className="space-y-6 pb-6">
          {/* Stats Grid */}
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <StatCard
              icon={<Download className="w-4 h-4" />}
              label="Total Downloads"
              value={String(stats.total)}
              color="text-primary"
              bgColor="bg-primary/10"
            />
            <StatCard
              icon={<Activity className="w-4 h-4" />}
              label="Active"
              value={String(stats.active)}
              color="text-blue-500"
              bgColor="bg-blue-500/10"
            />
            <StatCard
              icon={<CheckCircle2 className="w-4 h-4" />}
              label={t("download.completed")}
              value={String(stats.completed)}
              color="text-green-500"
              bgColor="bg-green-500/10"
            />
            <StatCard
              icon={<HardDrive className="w-4 h-4" />}
              label="Storage Used"
              value={formatSize(stats.totalSize)}
              color="text-orange-500"
              bgColor="bg-orange-500/10"
            />
          </div>

          {/* Quick Actions */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <QuickAction
              icon={<Download className="w-5 h-5" />}
              title={t("nav.download")}
              description="Download videos from URL"
              onClick={() => navigate("/download")}
            />
            <QuickAction
              icon={<Rss className="w-5 h-5" />}
              title={t("nav.rss")}
              description={`${feeds.length} subscriptions`}
              onClick={() => navigate("/rss")}
            />
            <QuickAction
              icon={<Mic className="w-5 h-5" />}
              title={t("nav.transcribe")}
              description="AI-powered transcription"
              onClick={() => navigate("/transcribe")}
            />
          </div>

          {/* Active Downloads */}
          {activeDownloads.length > 0 && (
            <Card>
              <CardContent className="p-4">
                <div className="flex items-center justify-between mb-3">
                  <h2 className="text-sm font-semibold flex items-center gap-2">
                    <Activity className="w-4 h-4 text-blue-500" />
                    Active Downloads
                  </h2>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-xs"
                    onClick={() => navigate("/download")}
                  >
                    View all
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
                    Recent Downloads
                  </h2>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-xs"
                    onClick={() => navigate("/download")}
                  >
                    View all
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
                  Start Downloading
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
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  color: string;
  bgColor: string;
}) {
  return (
    <Card>
      <CardContent className="p-4">
        <div className="flex items-center gap-3">
          <div
            className={`w-9 h-9 rounded-lg ${bgColor} flex items-center justify-center ${color}`}
          >
            {icon}
          </div>
          <div>
            <p className="text-lg font-bold">{value}</p>
            <p className="text-[11px] text-muted-foreground">{label}</p>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function QuickAction({
  icon,
  title,
  description,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="flex items-center gap-3 p-4 rounded-xl border border-border/50 bg-card hover:bg-accent transition-colors text-left group"
    >
      <div className="w-10 h-10 rounded-lg bg-primary/10 text-primary flex items-center justify-center group-hover:bg-primary group-hover:text-primary-foreground transition-colors">
        {icon}
      </div>
      <div>
        <p className="text-sm font-medium">{title}</p>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      <ArrowRight className="w-4 h-4 text-muted-foreground/30 ml-auto group-hover:text-foreground transition-colors" />
    </button>
  );
}

function ActiveDownloadRow({ download }: { download: DownloadItem }) {
  return (
    <div className="flex items-center gap-3">
      {download.thumbnail ? (
        <img
          src={download.thumbnail}
          alt=""
          className="w-14 h-9 rounded object-cover flex-shrink-0"
        />
      ) : (
        <div className="w-14 h-9 rounded bg-muted flex-shrink-0" />
      )}
      <div className="flex-1 min-w-0">
        <p className="text-xs font-medium truncate">{download.title}</p>
        <div className="flex items-center gap-2 mt-1">
          <Progress value={download.progress} className="h-1 flex-1" />
          <span className="text-[10px] text-muted-foreground flex-shrink-0">
            {Math.round(download.progress)}%
          </span>
        </div>
      </div>
    </div>
  );
}

function RecentDownloadRow({ download }: { download: DownloadItem }) {
  const statusIcon = () => {
    switch (download.status) {
      case "completed":
        return <CheckCircle2 className="w-3.5 h-3.5 text-green-500" />;
      case "error":
        return <AlertCircle className="w-3.5 h-3.5 text-destructive" />;
      case "downloading":
      case "queued":
        return <Activity className="w-3.5 h-3.5 text-blue-500" />;
      default:
        return <Clock className="w-3.5 h-3.5 text-muted-foreground" />;
    }
  };

  return (
    <div className="flex items-center gap-3 p-2 rounded-lg hover:bg-muted/50 transition-colors">
      {download.thumbnail ? (
        <img
          src={download.thumbnail}
          alt=""
          className="w-12 h-8 rounded object-cover flex-shrink-0"
        />
      ) : (
        <div className="w-12 h-8 rounded bg-muted flex-shrink-0" />
      )}
      <div className="flex-1 min-w-0">
        <p className="text-xs font-medium truncate">{download.title}</p>
        <p className="text-[10px] text-muted-foreground">
          {download.formatLabel || ""}
          {download.fileSize ? ` · ${formatSize(download.fileSize)}` : ""}
        </p>
      </div>
      {statusIcon()}
    </div>
  );
}

/* ─── Helpers ──────────────────────────────────────────── */

function formatSize(bytes: number): string {
  if (!bytes || bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}

export default DashboardPage;
