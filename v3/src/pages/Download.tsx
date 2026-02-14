import { useState, useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useAtom } from "jotai";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
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
import { formatBytes, formatDuration } from "@/lib/utils";
import { useDownloads } from "@/hooks/useDownloads";
import {
  videoInfoAtom,
  showQualityDialogAtom,
  pendingUrlAtom,
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
        toast.success(`Paused ${count} downloads`);
      }
    } catch (err) {
      toast.error(`Failed to pause downloads: ${err}`);
    }
  };

  const handleResumeAll = async () => {
    try {
      const count = await commands.resumeAllDownloads();
      if (count > 0) {
        toast.success(`Resumed ${count} downloads`);
      }
    } catch (err) {
      toast.error(`Failed to resume downloads: ${err}`);
    }
  };

  const handleCancelAll = async () => {
    try {
      const count = await commands.cancelAllDownloads();
      if (count > 0) {
        toast.success(`Cancelled ${count} downloads`);
      }
    } catch (err) {
      toast.error(`Failed to cancel downloads: ${err}`);
    }
  };

  const counts = {
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
  };

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
    } catch {
      // Error already toasted
    }
  }, [url, startDownload]);

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
      } catch {
        // Error toasted
      }
    },
    [pendingUrl, startDownload, setShowQuality],
  );

  return (
    <div className="flex flex-col h-full">
      {/* URL Input Section */}
      <div className="p-6 pb-4">
        <Card className="bg-card">
          <CardContent className="p-6">
            <h2 className="text-lg font-semibold mb-1">
              {t("download.title")}
            </h2>
            <p className="text-sm text-muted-foreground mb-4">
              {t("download.subtitle")}
            </p>

            {/* Download Type Tabs */}
            <Tabs
              value={downloadTab}
              onValueChange={(v) => setDownloadTab(v as DownloadTab)}
              className="mb-4"
            >
              <TabsList className="grid w-full grid-cols-2 gap-2">
                <TabsTrigger value="single" className="w-full">
                  <FileVideo className="w-4 h-4 mr-2" />
                  Single Video
                </TabsTrigger>
                <TabsTrigger value="playlist" className="w-full">
                  <List className="w-4 h-4 mr-2" />
                  Playlist
                </TabsTrigger>
              </TabsList>

              {/* Single Video Tab */}
              <TabsContent value="single" className="mt-4">
                {/* URL Input */}
                <div className="flex flex-col sm:flex-row gap-2 mb-4">
                  <Input
                    placeholder={t("download.placeholder")}
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    onKeyDown={(e) =>
                      e.key === "Enter" && handleQuickDownload()
                    }
                    className="flex-1 h-30 sm:h-11"
                  />
                  <div className="flex gap-2 shrink-0">
                    <Button
                      variant="outline"
                      size="default"
                      onClick={handlePaste}
                      className="h-11 flex-1 sm:flex-none px-4"
                    >
                      <ClipboardPaste className="w-4 h-4 sm:mr-2" />
                      <span className="sr-only sm:not-sr-only">
                        {t("download.paste")}
                      </span>
                    </Button>
                    <Button
                      variant="outline"
                      size="default"
                      onClick={handleFetchInfo}
                      disabled={!url.trim() || fetchingInfo}
                      className="h-11 flex-1 sm:flex-none px-4"
                    >
                      {fetchingInfo ? (
                        <Loader2 className="w-4 h-4 sm:mr-2 animate-spin" />
                      ) : (
                        <ChevronDown className="w-4 h-4 sm:mr-2" />
                      )}
                      <span className="sr-only sm:not-sr-only">
                        {t("download.selectQuality")}
                      </span>
                    </Button>
                    <Button
                      size="default"
                      onClick={handleQuickDownload}
                      disabled={!url.trim()}
                      className="h-11 flex-1 sm:flex-none px-6"
                    >
                      <DownloadIcon className="w-4 h-4 sm:mr-2" />
                      <span className="sr-only sm:not-sr-only">
                        {t("download.downloadNow")}
                      </span>
                    </Button>
                  </div>
                </div>
              </TabsContent>

              {/* Playlist Tab */}
              <TabsContent value="playlist" className="mt-4">
                <PlaylistDownload
                  onDownloadStart={() => setDownloadTab("single")}
                />
              </TabsContent>
            </Tabs>
          </CardContent>
        </Card>
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
      <div className="flex-1 px-6 pb-6 flex flex-col min-h-0">
        <Card className="flex-1 flex flex-col min-h-0">
          <div className="p-4 pb-3 border-b">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-base font-semibold">{t("download.queue")}</h3>
              {/* Batch Controls */}
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handlePauseAll}
                  disabled={counts.active === 0}
                >
                  <PauseCircle className="w-4 h-4 mr-1" />
                  Pause All
                </Button>
                <Button variant="outline" size="sm" onClick={handleResumeAll}>
                  <PlayCircle className="w-4 h-4 mr-1" />
                  Resume All
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleCancelAll}
                  disabled={counts.active === 0}
                >
                  <XCircle className="w-4 h-4 mr-1" />
                  Cancel All
                </Button>
              </div>
            </div>

            {/* Source Filter (Single / Playlist) */}
            <div className="flex items-center gap-2 mb-3">
              <Tabs
                value={sourceFilter}
                onValueChange={(v) => setSourceFilter(v as SourceFilter)}
              >
                <TabsList>
                  <TabsTrigger value="all">
                    All ({sourceCounts.all})
                  </TabsTrigger>
                  <TabsTrigger value="single">
                    <FileVideo className="w-3.5 h-3.5 mr-1.5" />
                    Single ({sourceCounts.single})
                  </TabsTrigger>
                  <TabsTrigger value="playlist">
                    <List className="w-3.5 h-3.5 mr-1.5" />
                    Playlist ({sourceCounts.playlist})
                  </TabsTrigger>
                </TabsList>
              </Tabs>
            </div>

            {/* Search Input + Status Filter Tabs inline */}
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 mb-3">
              <div className="shrink-0">
                <Tabs
                  value={filterTab}
                  onValueChange={(v) => setFilterTab(v as FilterTab)}
                >
                  <TabsList className="grid grid-cols-4 gap-2 max-w-md">
                    <TabsTrigger value="all">
                      {t("download.all")} ({counts.all})
                    </TabsTrigger>
                    <TabsTrigger value="active">
                      {t("download.active")} ({counts.active})
                    </TabsTrigger>
                    <TabsTrigger value="completed">
                      {t("download.completed")} ({counts.completed})
                    </TabsTrigger>
                    <TabsTrigger value="error">
                      {t("download.error")} ({counts.error})
                    </TabsTrigger>
                  </TabsList>
                </Tabs>
              </div>
              <div className="relative flex-1 w-full">
                <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                <Input
                  placeholder="Search downloads..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-9 w-full"
                />
              </div>
            </div>
          </div>

          <ScrollArea className="flex-1">
            <div className="p-4">
              {loading ? (
                <div className="flex items-center justify-center py-16">
                  <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
                </div>
              ) : filteredDownloads.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-16 text-center">
                  <DownloadIcon className="w-12 h-12 text-muted-foreground/30 mb-4" />
                  <p className="text-muted-foreground font-medium">
                    {t("download.noDownloads")}
                  </p>
                  <p className="text-sm text-muted-foreground/60 mt-1">
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
        </Card>
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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
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
                <span className="text-sm font-medium">Best quality (auto)</span>
                <Badge variant="secondary" className="ml-auto text-[10px]">
                  Recommended
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
                  No formats available in this category
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

function DownloadItemCard({
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
        return <Badge variant="success">{t("download.completed")}</Badge>;
      case "downloading":
        return <Badge variant="default">{t("download.downloading")}</Badge>;
      case "queued":
        return <Badge variant="secondary">{t("download.queued")}</Badge>;
      case "paused":
        return <Badge variant="warning">{t("download.paused")}</Badge>;
      case "error":
        return <Badge variant="destructive">{t("download.error")}</Badge>;
      case "cancelled":
        return <Badge variant="outline">{t("download.cancel")}</Badge>;
      case "merging":
        return <Badge variant="default">Merging...</Badge>;
      default:
        return <Badge variant="outline">{status}</Badge>;
    }
  };

  return (
    <div className="flex gap-3 sm:gap-4 p-3 sm:p-4 rounded-lg border bg-card hover:bg-accent/5 transition-colors">
      <div className="w-24 h-14 sm:w-40 sm:h-24 rounded-md bg-muted flex items-center justify-center flex-shrink-0 overflow-hidden">
        {download.thumbnail ? (
          <img
            src={download.thumbnail}
            alt={download.title}
            className="w-full h-full object-cover"
          />
        ) : (
          <FileVideo className="w-8 h-8 text-muted-foreground/40" />
        )}
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-start justify-between gap-2 mb-1">
          <h4 className="font-medium text-sm line-clamp-2">{download.title}</h4>
          {statusBadge(download.status)}
        </div>

        <div className="flex items-center gap-3 text-xs text-muted-foreground mb-2">
          <span>{download.createdAt}</span>
          {download.source === "playlist" && (
            <Badge variant="outline" className="text-[10px] px-1.5 py-0 gap-1">
              <List className="w-2.5 h-2.5" />
              Playlist
            </Badge>
          )}
          {download.formatLabel && (
            <Badge variant="outline" className="text-[10px] px-1.5 py-0">
              {download.formatLabel}
            </Badge>
          )}
          {download.fileSize != null && download.fileSize > 0 && (
            <span>{formatBytes(download.fileSize)}</span>
          )}
        </div>

        {(download.status === "downloading" ||
          download.status === "merging") && (
          <div className="space-y-1">
            <Progress value={download.progress} className="h-1.5" />
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{download.progress.toFixed(1)}%</span>
              <span>
                {download.speed && download.speed !== "0" && download.speed}{" "}
                {download.eta && download.eta !== "" && `ETA: ${download.eta}`}
              </span>
            </div>
          </div>
        )}

        {download.status === "error" && download.error && (
          <p className="text-xs text-destructive mt-1 line-clamp-1">
            {download.error}
          </p>
        )}
      </div>

      <div className="flex items-center gap-1 flex-shrink-0">
        {download.status === "completed" && download.filePath && (
          <>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={() => {
                import("@/lib/tauri").then(({ commands: cmd }) =>
                  cmd.openPath(download.filePath!),
                );
              }}
            >
              <ExternalLink className="w-4 h-4" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={() => {
                import("@/lib/tauri").then(({ commands: cmd }) => {
                  const dir = download.filePath!.replace(/[\\/][^\\/]+$/, "");
                  cmd.openPath(dir);
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
            className="h-8 w-8"
            onClick={() => onPause(download.id)}
          >
            <Pause className="w-4 h-4" />
          </Button>
        )}
        {download.status === "paused" && (
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => onResume(download.id)}
          >
            <Play className="w-4 h-4" />
          </Button>
        )}
        {download.status === "error" && (
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => onRetry(download.id)}
          >
            <RotateCcw className="w-4 h-4" />
          </Button>
        )}
        {(download.status === "downloading" ||
          download.status === "queued") && (
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => onCancel(download.id)}
          >
            <X className="w-4 h-4" />
          </Button>
        )}
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 text-destructive"
          onClick={() => onDelete(download.id, false)}
        >
          <Trash2 className="w-4 h-4" />
        </Button>
      </div>
    </div>
  );
}
