import { useState, useCallback, useMemo, memo, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useAtom, useAtomValue } from "jotai";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Download as DownloadIcon,
  ClipboardPaste,
  ExternalLink,
  FolderOpen,
  Trash2,
  RotateCcw,
  Pause,
  Play,
  X,
  FileVideo,
  Zap,
  Loader2,
  ChevronDown,
  Music,
  List,
  Search,
  PauseCircle,
  PlayCircle,
  XCircle,
} from "lucide-react";
import { formatBytes, formatDuration, cn } from "@/lib/utils";
import { useDownloads } from "@/hooks/useDownloads";
import {
  videoInfoAtom,
  showQualityDialogAtom,
  pendingUrlAtom,
  platformAtom,
} from "@/store/atoms";
import type {
  DownloadItem,
  DownloadStatus,
  VideoInfo,
  VideoFormat,
} from "@/lib/tauri";
import { PlaylistDownload } from "@/components/PlaylistDownload";
import { commands } from "@/lib/tauri";
import { toast } from "sonner";

type FilterTab = "all" | "active" | "completed" | "error";
type DownloadTab = "single" | "playlist";
type SourceFilter = "all" | "single" | "playlist";

export function DownloadPage() {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [fetchingInfo, setFetchingInfo] = useState(false);
  const [filterTab, setFilterTab] = useState<FilterTab>("all");
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>("all");
  const [downloadTab, setDownloadTab] = useState<DownloadTab>("single");
  const [videoInfo, setVideoInfo] = useAtom(videoInfoAtom);
  const [showQuality, setShowQuality] = useAtom(showQualityDialogAtom);
  const [pendingUrl, setPendingUrl] = useAtom(pendingUrlAtom);
  const platform = useAtomValue(platformAtom);

  const {
    downloads,
    loading,
    startDownload,
    pauseDownload,
    resumeDownload,
    cancelDownload,
    retryDownload,
    deleteDownload,
    getVideoInfo,
  } = useDownloads();

  // Search and filter downloads
  const filteredDownloads = useMemo(() => {
    let result = downloads;

    // Apply source filter
    if (sourceFilter !== "all") {
      result = result.filter((d) => (d.source || "single") === sourceFilter);
    }

    // Apply search filter
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      result = result.filter(
        (d) =>
          d.title.toLowerCase().includes(query) ||
          d.url.toLowerCase().includes(query),
      );
    }

    // Apply status filter
    switch (filterTab) {
      case "active":
        return result.filter(
          (d) =>
            d.status === "downloading" ||
            d.status === "queued" ||
            d.status === "merging",
        );
      case "completed":
        return result.filter((d) => d.status === "completed");
      case "error":
        return result.filter(
          (d) => d.status === "error" || d.status === "cancelled",
        );
      default:
        return result;
    }
  }, [downloads, searchQuery, filterTab, sourceFilter]);

  // Source counts
  const sourceCounts = useMemo(
    () => ({
      all: downloads.length,
      single: downloads.filter((d) => (d.source || "single") === "single")
        .length,
      playlist: downloads.filter((d) => d.source === "playlist").length,
    }),
    [downloads],
  );

  // Batch control functions
  const handlePauseAll = async () => {
    try {
      const count = await commands.pauseAllDownloads();
      if (count > 0) {
        toast.success(t("download.pausedCount", { count }));
      }
    } catch (err) {
      toast.error(t("download.pauseFailed", { error: String(err) }));
    }
  };

  const handleResumeAll = async () => {
    try {
      const count = await commands.resumeAllDownloads();
      if (count > 0) {
        toast.success(t("download.resumedCount", { count }));
      }
    } catch (err) {
      toast.error(t("download.resumeFailed", { error: String(err) }));
    }
  };

  const handleCancelAll = async () => {
    try {
      const count = await commands.cancelAllDownloads();
      if (count > 0) {
        toast.success(t("download.cancelledCount", { count }));
      }
    } catch (err) {
      toast.error(t("download.cancelFailed", { error: String(err) }));
    }
  };

  const counts = useMemo(
    () => ({
      all: downloads.length,
      active: downloads.filter(
        (d) =>
          d.status === "downloading" ||
          d.status === "queued" ||
          d.status === "merging",
      ).length,
      completed: downloads.filter((d) => d.status === "completed").length,
      error: downloads.filter(
        (d) => d.status === "error" || d.status === "cancelled",
      ).length,
    }),
    [downloads],
  );

  const handlePaste = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      setUrl(text);
    } catch {
      // Clipboard access denied
    }
  }, []);

  const handleQuickDownload = useCallback(async () => {
    if (!url.trim()) return;
    const downloadUrl = url.trim();
    setUrl("");
    try {
      await startDownload(downloadUrl);
      if (platform === "android") {
        toast.info(t("download.termuxStarted"));
      }
    } catch {
      // Error already toasted
    }
  }, [url, startDownload, platform, t]);

  const handleFetchInfo = useCallback(async () => {
    if (!url.trim()) return;
    setFetchingInfo(true);
    try {
      const info = await getVideoInfo(url.trim());
      setVideoInfo(info);
      setPendingUrl(url.trim());
      setShowQuality(true);
    } catch {
      // Fallback to quick download
      handleQuickDownload();
    } finally {
      setFetchingInfo(false);
    }
  }, [
    url,
    getVideoInfo,
    setVideoInfo,
    setPendingUrl,
    setShowQuality,
    handleQuickDownload,
  ]);

  const handleQualitySelect = useCallback(
    async (formatId: string) => {
      setShowQuality(false);
      const downloadUrl = pendingUrl;
      setUrl("");
      try {
        await startDownload(downloadUrl, formatId);
        if (platform === "android") {
          toast.info(t("download.termuxStarted"));
        }
      } catch {
        // Error toasted
      }
    },
    [pendingUrl, startDownload, setShowQuality, platform, t],
  );

  return (
    <div className="flex flex-col h-full">
      {/* URL Input Section */}
      <div className="p-4 sm:p-6 pb-2 sm:pb-4">
        <div className="relative overflow-hidden rounded-[24px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm p-5 sm:p-6">
          <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-transparent pointer-events-none" />
          <div className="relative z-10">
            <h2 className="text-xl sm:text-lg font-bold mb-1 tracking-tight">
              {t("download.title")}
            </h2>
            <p className="text-sm text-muted-foreground mb-5 font-medium">
              {t("download.subtitle")}
            </p>

            {/* Download Type Tabs */}
            <Tabs
              value={downloadTab}
              onValueChange={(v) => setDownloadTab(v as DownloadTab)}
            >
              <TabsList className="grid w-full grid-cols-2 gap-2 bg-muted p-1 rounded-full">
                <TabsTrigger
                  value="single"
                  className="w-full rounded-full data-[state=active]:bg-background data-[state=active]:shadow-sm"
                >
                  <FileVideo className="w-4 h-4 sm:mr-2" />
                  <span className="hidden sm:inline">
                    {t("download.singleVideo")}
                  </span>
                </TabsTrigger>
                <TabsTrigger
                  value="playlist"
                  className="w-full rounded-full data-[state=active]:bg-background data-[state=active]:shadow-sm"
                >
                  <List className="w-4 h-4 sm:mr-2" />
                  <span className="hidden sm:inline">
                    {t("download.playlist")}
                  </span>
                </TabsTrigger>
              </TabsList>

              {/* Single Video Tab */}
              <TabsContent
                value="single"
                className="mt-4 data-[state=active]:animate-in data-[state=active]:fade-in-50 data-[state=active]:slide-in-from-bottom-2"
              >
                {/* URL Input */}
                <div className="flex gap-2 items-center">
                  <div className="relative group flex-1">
                    <Input
                      placeholder={t("download.placeholder")}
                      value={url}
                      onChange={(e) => setUrl(e.target.value)}
                      onKeyDown={(e) =>
                        e.key === "Enter" && handleQuickDownload()
                      }
                      className="h-12 w-full rounded-full pr-12 bg-background/50 border-border/50 focus-visible:ring-primary/50 text-base"
                    />
                    {url && (
                      <button
                        onClick={() => setUrl("")}
                        className="absolute right-3 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground hover:bg-muted rounded-full transition-colors"
                      >
                        <X className="w-4 h-4" />
                      </button>
                    )}
                  </div>

                  <Button
                    variant="outline"
                    onClick={handlePaste}
                    className="h-12 w-12 sm:w-auto px-0 sm:px-4 rounded-full bg-background/50 border-border/50 hover:bg-muted shadow-sm whitespace-nowrap"
                  >
                    <ClipboardPaste className="w-5 h-5 sm:mr-2" />
                    <span className="hidden sm:inline">
                      {t("download.paste")}
                    </span>
                  </Button>
                  <Button
                    variant="outline"
                    onClick={handleFetchInfo}
                    disabled={!url.trim() || fetchingInfo}
                    className="h-12 w-12 sm:w-auto px-0 sm:px-4 rounded-full bg-background/50 border-border/50 hover:bg-muted shadow-sm whitespace-nowrap"
                  >
                    {fetchingInfo ? (
                      <Loader2 className="w-5 h-5 sm:mr-2 animate-spin" />
                    ) : (
                      <ChevronDown className="w-5 h-5 sm:mr-2" />
                    )}
                    <span className="hidden sm:inline">
                      {t("download.selectQuality")}
                    </span>
                  </Button>
                  <Button
                    onClick={handleQuickDownload}
                    disabled={!url.trim()}
                    className="h-12 w-12 sm:w-auto px-0 sm:px-4 rounded-full shadow-sm text-base font-medium whitespace-nowrap"
                  >
                    <DownloadIcon className="w-5 h-5 sm:mr-2" />
                    <span className="hidden sm:inline">
                      {t("download.downloadNow")}
                    </span>
                  </Button>
                </div>
              </TabsContent>

              {/* Playlist Tab */}
              <TabsContent value="playlist" className="mt-4">
                <PlaylistDownload
                  onDownloadStart={() => setDownloadTab("single")}
                />
              </TabsContent>
            </Tabs>
          </div>
        </div>
      </div>

      {/* Quality Selection Dialog */}
      {showQuality && videoInfo && (
        <QualityDialog
          info={videoInfo}
          onSelect={handleQualitySelect}
          onClose={() => setShowQuality(false)}
        />
      )}

      {/* Download Queue Section */}
      <div className="flex-1 px-4 sm:px-6 pb-6 flex flex-col min-h-0">
        <div className="flex-1 flex flex-col min-h-0 bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 rounded-[24px] shadow-sm overflow-hidden">
          <div className="p-4 sm:p-5 border-b bg-muted/20">
            <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4 mb-4">
              <div className="flex items-center gap-4">
                <h3 className="text-lg font-bold tracking-tight whitespace-nowrap">
                  {t("download.queue")}
                </h3>

                {/* Source Filter (Single / Playlist) */}
                <div className="w-full overflow-x-auto pb-1 sm:pb-0 sm:w-auto scrollbar-hide">
                  <Tabs
                    value={sourceFilter}
                    onValueChange={(v) => setSourceFilter(v as SourceFilter)}
                    className="w-full sm:w-auto min-w-max"
                  >
                    <TabsList className="bg-background/80 border border-border/50 rounded-full h-9 inline-flex p-1">
                      <TabsTrigger
                        value="all"
                        className="rounded-full text-xs px-3 data-[state=active]:shadow-sm"
                      >
                        {t("download.all")} ({sourceCounts.all})
                      </TabsTrigger>
                      <TabsTrigger
                        value="single"
                        className="rounded-full text-xs px-3 data-[state=active]:shadow-sm flex items-center"
                      >
                        <FileVideo className="w-3.5 h-3.5 sm:mr-1.5" />
                        <span className="hidden sm:inline">
                          {t("download.single")}
                        </span>
                        <span className="ml-1 sm:ml-0">
                          ({sourceCounts.single})
                        </span>
                      </TabsTrigger>
                      <TabsTrigger
                        value="playlist"
                        className="rounded-full text-xs px-3 data-[state=active]:shadow-sm flex items-center"
                      >
                        <List className="w-3.5 h-3.5 sm:mr-1.5" />
                        <span className="hidden sm:inline">
                          {t("download.playlist")}
                        </span>
                        <span className="ml-1 sm:ml-0">
                          ({sourceCounts.playlist})
                        </span>
                      </TabsTrigger>
                    </TabsList>
                  </Tabs>
                </div>

                <div className="flex gap-1.5 sm:gap-2 ml-auto">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handlePauseAll}
                    disabled={counts.active === 0}
                    className="rounded-full h-8 px-2 sm:px-3 text-xs"
                  >
                    <PauseCircle className="w-3.5 h-3.5 sm:mr-1.5" />
                    <span className="hidden sm:inline">
                      {t("download.pauseAll")}
                    </span>
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleResumeAll}
                    className="rounded-full h-8 px-2 sm:px-3 text-xs"
                  >
                    <PlayCircle className="w-3.5 h-3.5 sm:mr-1.5" />
                    <span className="hidden sm:inline">
                      {t("download.resumeAll")}
                    </span>
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleCancelAll}
                    disabled={counts.active === 0}
                    className="rounded-full h-8 px-2 sm:px-3 text-xs"
                  >
                    <XCircle className="w-3.5 h-3.5 sm:mr-1.5" />
                    <span className="hidden sm:inline">
                      {t("download.cancelAll")}
                    </span>
                  </Button>
                </div>
              </div>
            </div>

            {/* Search Input + Status Filter Tabs */}
            <div className="flex items-center gap-3">
              <div className="overflow-x-auto scrollbar-hide">
                <Tabs
                  value={filterTab}
                  onValueChange={(v) => setFilterTab(v as FilterTab)}
                  className="w-full sm:w-auto min-w-max"
                >
                  <TabsList className="bg-background/80 border border-border/50 rounded-full h-9 inline-flex p-1">
                    <TabsTrigger
                      value="all"
                      className="rounded-full text-xs px-3 data-[state=active]:shadow-sm"
                    >
                      {t("download.all")} ({counts.all})
                    </TabsTrigger>
                    <TabsTrigger
                      value="active"
                      className="rounded-full text-xs px-3 data-[state=active]:shadow-sm"
                    >
                      {t("download.active")} ({counts.active})
                    </TabsTrigger>
                    <TabsTrigger
                      value="completed"
                      className="rounded-full text-xs px-3 data-[state=active]:shadow-sm"
                    >
                      {t("download.completed")} ({counts.completed})
                    </TabsTrigger>
                    <TabsTrigger
                      value="error"
                      className="rounded-full text-xs px-3 data-[state=active]:shadow-sm"
                    >
                      {t("download.error")} ({counts.error})
                    </TabsTrigger>
                  </TabsList>
                </Tabs>
              </div>

              <div
                className={`relative transition-all duration-300 ${searchQuery ? "flex-1 w-full" : "w-9 sm:flex-1"} min-w-0 sm:min-w-[140px] group`}
              >
                <Search
                  className={`absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground ${searchQuery ? "opacity-100" : "opacity-100 sm:opacity-100"} transition-opacity`}
                />
                <Input
                  placeholder={t("download.searchPlaceholder")}
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className={`pl-9 h-9 rounded-full bg-background/80 border-border/50 text-sm focus-visible:ring-primary/30 transition-all duration-300
                    ${searchQuery ? "w-full" : "w-9 sm:w-full opacity-0 sm:opacity-100 cursor-pointer sm:cursor-text focus:w-[200px] focus:opacity-100 focus:cursor-text"}
                  `}
                />
              </div>
            </div>
          </div>

          <ScrollArea className="flex-1 bg-card/50">
            <div className="p-3 sm:p-4">
              {loading ? (
                <div className="flex items-center justify-center py-16">
                  <Loader2 className="w-8 h-8 animate-spin text-muted-foreground/50" />
                </div>
              ) : filteredDownloads.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-20 text-center">
                  <div className="w-16 h-16 rounded-2xl bg-muted flex items-center justify-center mb-4">
                    <DownloadIcon className="w-8 h-8 text-muted-foreground/40" />
                  </div>
                  <p className="text-muted-foreground font-semibold">
                    {t("download.noDownloads")}
                  </p>
                  <p className="text-sm text-muted-foreground/60 mt-1 max-w-[250px]">
                    {t("download.noDownloadsDesc")}
                  </p>
                </div>
              ) : (
                <div className="space-y-3">
                  {filteredDownloads.map((dl) => (
                    <DownloadItemCard
                      key={dl.id}
                      download={dl}
                      onPause={pauseDownload}
                      onResume={resumeDownload}
                      onCancel={cancelDownload}
                      onRetry={retryDownload}
                      onDelete={deleteDownload}
                    />
                  ))}
                </div>
              )}
            </div>
          </ScrollArea>
        </div>
      </div>
    </div>
  );
}

// ─── Quality Selection Dialog ────────────────────────────────────────

type MergedFormat = VideoFormat & {
  merged?: boolean;
  audioFormatId?: string;
};

function QualityDialog({
  info,
  onSelect,
  onClose,
}: {
  info: VideoInfo;
  onSelect: (formatId: string) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"combined" | "video" | "audio">("combined");
  const dialogRef = useRef<HTMLDivElement>(null);

  // Focus trap + Escape key handler
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    // Focus the dialog container on mount for keyboard navigation
    dialogRef.current?.focus();
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  // Get helper function to extract resolution height
  const getHeight = (res: string, fmt?: VideoFormat): number => {
    // Prefer numeric height from yt-dlp when available (reliable for 4K+)
    if (fmt?.height && fmt.height > 0) return fmt.height;
    // Handle "1920x1080" format
    const dimMatch = res.match(/(\d+)x(\d+)/);
    if (dimMatch) {
      return parseInt(dimMatch[2] || "0");
    }
    // Handle "1080p" format
    const pMatch = res.match(/(\d+)p/);
    if (pMatch) {
      return parseInt(pMatch[1] || "0");
    }
    return 0;
  };

  // Convert raw resolution to standard label
  const formatResolution = (res: string, fmt?: VideoFormat): string => {
    const height = getHeight(res, fmt);
    if (height >= 2160) return "4K";
    if (height >= 1440) return "1440p";
    if (height >= 1080) return "1080p";
    if (height >= 720) return "720p";
    if (height >= 480) return "480p";
    if (height >= 360) return "360p";
    if (height > 0) return `${height}p`;
    return res; // Fallback to original
  };

  // Native combined formats (have both video and audio)
  const nativeCombined = info.formats.filter(
    (f) => f.vcodec !== "none" && f.acodec !== "none",
  );

  // Video-only formats (DASH video streams)
  const videoOnlyFormats = info.formats
    .filter((f) => f.vcodec !== "none" && f.acodec === "none")
    .sort((a, b) => getHeight(b.resolution, b) - getHeight(a.resolution, a));

  // Audio-only formats (DASH audio streams)
  const audioOnlyFormats = info.formats
    .filter((f) => f.vcodec === "none" && f.acodec !== "none")
    .sort((a, b) => (b.tbr || 0) - (a.tbr || 0));

  // Find best audio format for merging
  const bestAudio = audioOnlyFormats[0];

  // Create merged format options: video-only + best audio
  const mergedFormats: MergedFormat[] = bestAudio
    ? videoOnlyFormats.map((f) => ({
        ...f,
        formatId: `${f.formatId}+${bestAudio.formatId}`,
        merged: true,
        audioFormatId: bestAudio.formatId,
        // Estimate combined filesize
        filesize:
          f.filesize && bestAudio.filesize
            ? f.filesize + bestAudio.filesize
            : f.filesize,
      }))
    : [];

  // Combined = merged + native combined (deduplicate by resolution height only)
  const seenHeights = new Set<number>();
  const combinedFormats: MergedFormat[] = [];

  // First add merged formats (higher quality DASH streams)
  for (const fmt of mergedFormats) {
    const height = getHeight(fmt.resolution, fmt);
    if (!seenHeights.has(height)) {
      seenHeights.add(height);
      combinedFormats.push(fmt);
    }
  }

  // Then add native combined formats that have different heights
  for (const fmt of nativeCombined) {
    const height = getHeight(fmt.resolution, fmt);
    if (!seenHeights.has(height)) {
      seenHeights.add(height);
      combinedFormats.push(fmt as MergedFormat);
    }
  }

  // Sort by resolution descending
  combinedFormats.sort(
    (a, b) => getHeight(b.resolution, b) - getHeight(a.resolution, a),
  );

  const currentFormats: MergedFormat[] =
    tab === "combined"
      ? combinedFormats
      : tab === "video"
        ? videoOnlyFormats
        : audioOnlyFormats;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label={t("download.selectQuality")}
      ref={dialogRef}
      tabIndex={-1}
    >
      <Card className="w-full max-w-[680px] m-4 max-h-[560px] flex flex-col shadow-2xl">
        <CardContent className="p-0 flex flex-col max-h-[560px]">
          {/* Header */}
          <div className="p-4 border-b">
            <div className="flex items-start gap-3">
              {info.thumbnail && (
                <img
                  src={info.thumbnail}
                  alt={info.title}
                  className="w-24 h-14 rounded object-cover flex-shrink-0"
                />
              )}
              <div className="min-w-0 flex-1">
                <h3 className="font-semibold text-sm line-clamp-2">
                  {info.title}
                </h3>
                <p className="text-xs text-muted-foreground mt-0.5">
                  {info.uploader} •{" "}
                  {info.duration ? formatDuration(info.duration) : ""}
                </p>
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="ml-auto -mr-2 -mt-1"
                onClick={onClose}
              >
                <X className="w-4 h-4" />
              </Button>
            </div>

            <Tabs
              value={tab}
              onValueChange={(v) => setTab(v as typeof tab)}
              className="mt-3"
            >
              <TabsList className="grid w-full grid-cols-3 gap-2 sticky top-0 bg-card z-10">
                <TabsTrigger value="combined" className="w-full">
                  {t("download.video")} + Audio ({combinedFormats.length})
                </TabsTrigger>
                <TabsTrigger value="video" className="w-full">
                  {t("download.video")} ({videoOnlyFormats.length})
                </TabsTrigger>
                <TabsTrigger value="audio" className="w-full">
                  {t("download.audio")} ({audioOnlyFormats.length})
                </TabsTrigger>
              </TabsList>
            </Tabs>
          </div>

          {/* Format list */}
          <ScrollArea key={tab} className="flex-1 h-72 overflow-auto">
            <div className="p-2 space-y-1">
              <button
                onClick={() => onSelect("best")}
                className="w-full flex items-center gap-3 p-2.5 rounded-lg hover:bg-accent transition-colors text-left"
              >
                <Zap className="w-4 h-4 text-primary flex-shrink-0" />
                <span className="text-sm font-medium">
                  {t("download.bestQualityAuto")}
                </span>
                <Badge variant="secondary" className="ml-auto text-[10px]">
                  {t("download.recommended")}
                </Badge>
              </button>

              {currentFormats.map((fmt) => (
                <button
                  key={fmt.formatId}
                  onClick={() => onSelect(fmt.formatId)}
                  className="w-full flex items-center gap-3 p-2.5 rounded-lg hover:bg-accent transition-colors text-left"
                >
                  {tab === "audio" ? (
                    <Music className="w-4 h-4 text-muted-foreground flex-shrink-0" />
                  ) : (
                    <FileVideo className="w-4 h-4 text-muted-foreground flex-shrink-0" />
                  )}
                  <div className="flex-1 min-w-0">
                    <span className="text-sm font-medium">
                      {tab === "audio"
                        ? `${fmt.tbr ? Math.round(fmt.tbr) + "kbps" : ""} • ${fmt.ext.toUpperCase()}`
                        : `${formatResolution(fmt.resolution, fmt)} • ${fmt.ext.toUpperCase()}`}
                    </span>
                    {fmt.formatNote && (
                      <span className="text-xs text-muted-foreground ml-2">
                        {fmt.formatNote}
                      </span>
                    )}
                  </div>
                  <div className="text-right flex-shrink-0 text-xs text-muted-foreground space-x-2">
                    {fmt.fps != null && <span>{fmt.fps}fps</span>}
                    {fmt.filesize != null && (
                      <span>{formatBytes(fmt.filesize)}</span>
                    )}
                  </div>
                  {/* Codec and HDR badges */}
                  <div className="flex-shrink-0 flex gap-1">
                    {fmt.vcodec && fmt.vcodec !== "none" && (
                      <Badge variant="outline" className="text-[9px] py-0 px-1">
                        {fmt.vcodec.includes("av01")
                          ? "AV1"
                          : fmt.vcodec.includes("vp9")
                            ? "VP9"
                            : fmt.vcodec.includes("vp8")
                              ? "VP8"
                              : fmt.vcodec.includes("avc1")
                                ? "H.264"
                                : fmt.vcodec.includes("hev1") ||
                                    fmt.vcodec.includes("hvc1")
                                  ? "HEVC"
                                  : fmt.vcodec.split(".")[0].toUpperCase()}
                      </Badge>
                    )}
                    {fmt.formatNote?.toLowerCase().includes("hdr") && (
                      <Badge
                        variant="default"
                        className="text-[9px] py-0 px-1 bg-amber-500"
                      >
                        HDR
                      </Badge>
                    )}
                  </div>
                </button>
              ))}

              {currentFormats.length === 0 && (
                <p className="text-center text-sm text-muted-foreground py-4">
                  {t("download.noFormats")}
                </p>
              )}
            </div>
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}

// ─── Download Item Card ──────────────────────────────────────────────

const DownloadItemCard = memo(function DownloadItemCard({
  download,
  onPause,
  onResume,
  onCancel,
  onRetry,
  onDelete,
}: {
  download: DownloadItem;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onRetry: (id: string) => void;
  onDelete: (id: string, deleteFile: boolean) => void;
}) {
  const { t } = useTranslation();

  const statusBadge = (status: DownloadStatus) => {
    switch (status) {
      case "completed":
        return (
          <span className="text-[10px] font-medium text-emerald-500 bg-emerald-500/10 px-2 py-0.5 rounded-full">
            {t("download.completed")}
          </span>
        );
      case "downloading":
        return (
          <span className="text-[10px] font-medium text-primary bg-primary/10 px-2 py-0.5 rounded-full">
            {t("download.downloading")}
          </span>
        );
      case "queued":
        return (
          <span className="text-[10px] font-medium text-muted-foreground bg-muted px-2 py-0.5 rounded-full">
            {t("download.queued")}
          </span>
        );
      case "paused":
        return (
          <span className="text-[10px] font-medium text-amber-500 bg-amber-500/10 px-2 py-0.5 rounded-full">
            {t("download.paused")}
          </span>
        );
      case "error":
        return (
          <span className="text-[10px] font-medium text-destructive bg-destructive/10 px-2 py-0.5 rounded-full">
            {t("download.error")}
          </span>
        );
      case "cancelled":
        return (
          <span className="text-[10px] font-medium text-muted-foreground bg-muted px-2 py-0.5 rounded-full">
            {t("download.cancel")}
          </span>
        );
      case "merging":
        return (
          <span className="text-[10px] font-medium text-purple-500 bg-purple-500/10 px-2 py-0.5 rounded-full">
            {t("download.merging")}
          </span>
        );
      default:
        return (
          <span className="text-[10px] font-medium text-muted-foreground bg-muted px-2 py-0.5 rounded-full">
            {status}
          </span>
        );
    }
  };

  return (
    <div className="flex flex-col sm:flex-row p-2.5 sm:p-3 rounded-2xl bg-background/50 backdrop-blur-md border border-border/50 dark:border-white/10 hover:bg-accent/20 transition-colors shadow-sm w-full overflow-hidden">
      <div className="flex gap-3 sm:gap-4 w-full min-w-0">
        <div className="relative w-24 h-16 sm:w-32 sm:h-20 rounded-xl overflow-hidden flex-shrink-0 bg-muted/50 border border-border/50 dark:border-white/10 shadow-sm">
          {download.thumbnail ? (
            <img
              src={download.thumbnail}
              alt={download.title}
              className="w-full h-full object-cover"
            />
          ) : (
            <div className="flex items-center justify-center w-full h-full">
              <FileVideo className="w-8 h-8 text-muted-foreground/40" />
            </div>
          )}
          {/* subtle gradient overlay */}
          <div className="absolute inset-0 bg-gradient-to-t from-black/40 to-transparent" />
        </div>

        <div className="flex-1 min-w-0 flex flex-col pt-1">
          <div className="flex items-start justify-between gap-2 mb-1.5">
            <h4 className="font-semibold text-sm leading-tight line-clamp-2 flex-1 break-words">
              {download.title}
            </h4>
            <div className="flex items-center gap-0.5 sm:gap-1 flex-shrink-0">
              <ActionButtons
                download={download}
                onPause={onPause}
                onResume={onResume}
                onCancel={onCancel}
                onRetry={onRetry}
                onDelete={onDelete}
              />
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground mb-2 mt-1.5">
            {statusBadge(download.status)}

            {download.formatLabel && (
              <span className="font-medium">{download.formatLabel}</span>
            )}
            {download.fileSize != null && download.fileSize > 0 && (
              <span className="font-medium">
                • {formatBytes(download.fileSize)}
              </span>
            )}
            {download.source === "playlist" && (
              <span className="flex items-center gap-1 text-primary bg-primary/10 px-1.5 py-0.5 rounded font-medium">
                <List className="w-3 h-3" />
                {t("download.playlist")}
              </span>
            )}
          </div>

          {(download.status === "downloading" ||
            download.status === "merging") && (
            <div className="space-y-1.5 mt-1">
              <div className="flex-1 h-1.5 bg-muted/50 rounded-full overflow-hidden">
                {/* Termux downloads have no progress tracking — show indeterminate animation */}
                {download.title.startsWith("Termux:") &&
                download.progress === 0 ? (
                  <div className="h-full bg-primary rounded-full animate-pulse w-full opacity-50" />
                ) : (
                  <div
                    className={cn(
                      "h-full rounded-full transition-all duration-300",
                      download.status === "merging"
                        ? "bg-purple-500 animate-pulse"
                        : "bg-primary",
                    )}
                    style={{ width: `${download.progress}%` }}
                  />
                )}
              </div>
              <div className="flex items-center justify-between text-[10px] font-medium text-muted-foreground">
                <span className="flex gap-2">
                  {download.title.startsWith("Termux:") &&
                  download.progress === 0 ? (
                    <span className="text-foreground">
                      {t("download.termuxRunning", "Running in Termux...")}
                    </span>
                  ) : (
                    <>
                      <span className="text-foreground">
                        {download.progress.toFixed(1)}%
                      </span>
                      {download.speed && download.speed !== "0" && (
                        <span>{download.speed}</span>
                      )}
                    </>
                  )}
                </span>
                <span>
                  {download.eta &&
                    download.eta !== "" &&
                    `ETA: ${download.eta}`}
                </span>
              </div>
            </div>
          )}

          {download.status === "error" && download.error && (
            <p className="text-[11px] text-destructive mt-1 font-medium line-clamp-1 bg-destructive/10 px-2 py-1 rounded inline-block self-start">
              {download.error}
            </p>
          )}
        </div>
      </div>
    </div>
  );
});

function ActionButtons({
  download,
  onPause,
  onResume,
  onCancel,
  onRetry,
  onDelete,
}: {
  download: DownloadItem;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onRetry: (id: string) => void;
  onDelete: (id: string, deleteFile: boolean) => void;
}) {
  return (
    <>
      {download.status === "completed" && download.filePath && (
        <>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 rounded-full hover:bg-background/80"
            onClick={() => {
              commands.openPath(download.filePath!).catch((err) => {
                toast.error(`Failed to open file: ${String(err)}`);
              });
            }}
          >
            <ExternalLink className="w-4 h-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 rounded-full hover:bg-background/80"
            onClick={() => {
              const dir = download.filePath!.replace(/[\\/][^\\/]+$/, "");
              commands.openPath(dir).catch((err) => {
                toast.error(`Failed to open folder: ${String(err)}`);
              });
            }}
          >
            <FolderOpen className="w-4 h-4" />
          </Button>
        </>
      )}
      {download.status === "downloading" && (
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 rounded-full bg-background/50 hover:bg-background shadow-sm border border-border/50 text-amber-500"
          onClick={() => onPause(download.id)}
        >
          <Pause className="w-4 h-4" />
        </Button>
      )}
      {download.status === "paused" && (
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 rounded-full bg-background/50 hover:bg-background shadow-sm border border-border/50 text-emerald-500"
          onClick={() => onResume(download.id)}
        >
          <Play className="w-4 h-4" />
        </Button>
      )}
      {download.status === "error" && (
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 rounded-full bg-background/50 hover:bg-background shadow-sm border border-border/50 text-primary"
          onClick={() => onRetry(download.id)}
        >
          <RotateCcw className="w-4 h-4" />
        </Button>
      )}
      {(download.status === "downloading" || download.status === "queued") && (
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 rounded-full hover:bg-destructive/10 text-destructive"
          onClick={() => onCancel(download.id)}
        >
          <X className="w-4 h-4" />
        </Button>
      )}
      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 rounded-full hover:bg-destructive/10 text-destructive"
        onClick={() => onDelete(download.id, true)}
        title="Delete file"
      >
        <Trash2 className="w-4 h-4" />
      </Button>
    </>
  );
}
