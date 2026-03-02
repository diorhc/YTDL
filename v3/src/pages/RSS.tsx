import { memo, useEffect, useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
} from "@/components/ui/tooltip";
import {
  Plus,
  Rss as RssIcon,
  RefreshCw,
  Trash2,
  Loader2,
  Download,
  DownloadCloud,
  Play,
  Smartphone,
  Clock,
  Filter,
  CheckCircle2,
  Video,
} from "lucide-react";
import { useRss } from "@/hooks/useRss";
import type { RssFeed, RssItem } from "@/lib/tauri";
import { commands, events, type RssSyncProgressEvent } from "@/lib/tauri";
import { toast } from "sonner";
import { VideoPlayer } from "@/components/VideoPlayer";

export function RssPage() {
  const { t } = useTranslation();
  const { feeds, loading, addFeed, removeFeed, checkFeed } = useRss();
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [selectedFeedId, setSelectedFeedId] = useState<string | null>(null);
  const [authorFilterId, setAuthorFilterId] = useState<string>("all");
  const [visibleCount, setVisibleCount] = useState(120);
  const [videoTypeFilter, setVideoTypeFilter] = useState<
    "all" | "video" | "short"
  >("all");
  const [syncProgress, setSyncProgress] = useState<
    Record<string, RssSyncProgressEvent>
  >({});

  // Player state
  const [playerUrl, setPlayerUrl] = useState<string | null>(null);
  const [playerTitle, setPlayerTitle] = useState("");
  const [playerIsShort, setPlayerIsShort] = useState(false);

  // Selected feed for detail view
  const selectedFeed = useMemo(
    () => feeds.find((f) => f.id === selectedFeedId) || feeds[0] || null,
    [feeds, selectedFeedId],
  );

  const scopedFeeds = useMemo(() => {
    if (selectedFeedId) {
      return feeds.filter((f) => f.id === selectedFeedId);
    }
    if (authorFilterId !== "all") {
      return feeds.filter((f) => f.id === authorFilterId);
    }
    return feeds;
  }, [feeds, selectedFeedId, authorFilterId]);

  const visibleItems = useMemo(() => {
    let items = scopedFeeds
      .flatMap((f) =>
        (f.items || []).map((item) => ({
          ...item,
          feedId: f.id,
          feedTitle: f.channelName || f.title,
          feedAvatar: f.channelAvatar,
        })),
      )
      .sort(
        (a, b) =>
          new Date(b.publishedAt).getTime() - new Date(a.publishedAt).getTime(),
      );

    if (videoTypeFilter !== "all") {
      items = items.filter((item) => inferVideoType(item) === videoTypeFilter);
    }

    return items;
  }, [scopedFeeds, videoTypeFilter]);

  const newTopItems = useMemo(
    () => visibleItems.filter((i) => i.status === "not_queued"),
    [visibleItems],
  );

  const renderedItems = useMemo(
    () => visibleItems.slice(0, visibleCount),
    [visibleItems, visibleCount],
  );

  useEffect(() => {
    setVisibleCount(120);
  }, [selectedFeedId, authorFilterId, videoTypeFilter]);

  const authorOptions = useMemo(
    () =>
      feeds.map((feed) => ({
        id: feed.id,
        name: feed.channelName || feed.title || "Channel",
        count: feed.items?.length || 0,
      })),
    [feeds],
  );

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    events
      .onRssSyncProgress((payload) => {
        setSyncProgress((prev) => ({ ...prev, [payload.feedId]: payload }));

        if (payload.phase === "completed" || payload.phase === "error") {
          window.setTimeout(() => {
            setSyncProgress((prev) => {
              if (!prev[payload.feedId]) return prev;
              const next = { ...prev };
              delete next[payload.feedId];
              return next;
            });
          }, 3500);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const selectedAuthorLabel = useMemo(() => {
    if (selectedFeedId) {
      return selectedFeed?.channelName || selectedFeed?.title || "Channel";
    }
    if (authorFilterId === "all") return t("rss.allAuthors");
    return (
      authorOptions.find((option) => option.id === authorFilterId)?.name ||
      t("rss.allAuthors")
    );
  }, [authorFilterId, authorOptions, selectedFeed, selectedFeedId, t]);

  // Counts for filter badges
  const typeCounts = useMemo(() => {
    const allItems = scopedFeeds.flatMap((f) => f.items || []);
    return {
      all: allItems.length,
      video: allItems.filter((i) => inferVideoType(i) === "video").length,
      short: allItems.filter((i) => inferVideoType(i) === "short").length,
    };
  }, [scopedFeeds]);

  const handleDownloadAllNewTop = async () => {
    if (newTopItems.length === 0) return;
    const urls = newTopItems.map((item) => item.url).filter(Boolean);
    const results = await Promise.allSettled(
      urls.map((itemUrl) => commands.startDownload(itemUrl)),
    );
    const started = results.filter(
      (result) => result.status === "fulfilled",
    ).length;

    results.forEach((result, index) => {
      if (result.status === "rejected") {
        console.error(
          `Failed to start download for ${newTopItems[index]?.title}:`,
          result.reason,
        );
      }
    });

    if (started > 0)
      toast.success(t("rss.startedDownloads", { count: started }));
  };

  const handleRefreshAll = async () => {
    try {
      toast.info(t("rss.checkNow") + "...");
      const count = await commands.checkAllRssFeeds();
      toast.success(t("rss.updatedFeeds", { count }));
    } catch (err) {
      toast.error(t("rss.refreshFailed", { error: String(err) }));
    }
  };

  const openPlayer = (url: string, title: string, isShort: boolean) => {
    setPlayerUrl(url);
    setPlayerTitle(title);
    setPlayerIsShort(isShort);
  };

  const closePlayer = () => {
    setPlayerUrl(null);
    setPlayerTitle("");
    setPlayerIsShort(false);
  };

  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex flex-col h-full bg-background">
        {/* Header */}
        <div className="flex items-center justify-between px-4 sm:px-6 pt-6 pb-2 sm:pb-4">
          <div>
            <h1 className="text-2xl sm:text-3xl font-bold tracking-tight">
              {t("rss.title")}
            </h1>
            <p className="text-sm text-muted-foreground font-medium mt-1">
              {t("rss.subtitle")}
            </p>
          </div>
          {feeds.length > 0 && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="outline"
                  size="icon"
                  className="h-10 w-10 sm:h-11 sm:w-11 rounded-full shadow-sm border-border/50 hover:bg-muted"
                  onClick={handleRefreshAll}
                >
                  <RefreshCw className="w-4 h-4 sm:w-5 sm:h-5 text-primary" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>{t("rss.refreshAll")}</TooltipContent>
            </Tooltip>
          )}
        </div>

        {/* Channel Tabs */}
        <div className="px-4 sm:px-6 pb-2">
          <ScrollArea className="w-full whitespace-nowrap pb-3 scrollbar-hide">
            <div className="flex items-start gap-4 sm:gap-5">
              {/* All feeds tab */}
              <button
                onClick={() => setSelectedFeedId(null)}
                className={`flex flex-col items-center gap-2 min-w-[64px] transition-all group ${
                  !selectedFeedId
                    ? "opacity-100 scale-100"
                    : "opacity-60 hover:opacity-100 scale-95 hover:scale-100"
                }`}
              >
                <div
                  className={`w-14 h-14 sm:w-16 sm:h-16 rounded-full flex items-center justify-center transition-all shadow-sm ${
                    !selectedFeedId
                      ? "bg-gradient-to-br from-primary to-primary/80 text-primary-foreground shadow-primary/20"
                      : "bg-muted/50 border border-white/5 group-hover:bg-muted"
                  }`}
                >
                  <RssIcon
                    className={`w-6 h-6 ${!selectedFeedId ? "text-white" : "text-muted-foreground"}`}
                  />
                </div>
                <span className="text-[11px] font-semibold truncate w-full text-center px-1">
                  {t("download.all")}
                </span>
              </button>

              {/* Channel tabs */}
              {feeds.map((feed) => (
                <ChannelTab
                  key={feed.id}
                  feed={feed}
                  isActive={selectedFeedId === feed.id}
                  syncEntry={syncProgress[feed.id]}
                  onClick={() => setSelectedFeedId(feed.id)}
                  onRefresh={() => checkFeed(feed.id)}
                  onRemove={() => {
                    removeFeed(feed.id);
                    if (selectedFeedId === feed.id) setSelectedFeedId(null);
                  }}
                />
              ))}

              {/* Add channel button */}
              <button
                onClick={() => setShowAddDialog(true)}
                className="flex flex-col items-center gap-2 min-w-[64px] opacity-60 hover:opacity-100 transition-all group scale-95 hover:scale-100"
              >
                <div className="w-14 h-14 sm:w-16 sm:h-16 rounded-full border-2 border-dashed border-muted-foreground/30 flex items-center justify-center group-hover:border-primary/50 group-hover:bg-primary/5 transition-all">
                  <Plus className="w-6 h-6 text-muted-foreground group-hover:text-primary transition-colors" />
                </div>
                <span className="text-[11px] font-semibold text-muted-foreground group-hover:text-primary transition-colors">
                  {t("common.add")}
                </span>
              </button>
            </div>
            <ScrollBar orientation="horizontal" className="hidden" />
          </ScrollArea>
        </div>

        {/* Content area */}
        <ScrollArea className="flex-1 px-4 sm:px-6">
          {loading ? (
            <div className="flex items-center justify-center py-20">
              <Loader2 className="w-10 h-10 animate-spin text-muted-foreground/50" />
            </div>
          ) : feeds.length === 0 ? (
            <EmptyState onAdd={() => setShowAddDialog(true)} />
          ) : selectedFeed ? (
            <>
              {/* Filter tabs + bulk action */}
              <div className="mb-5 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div className="flex items-center gap-2 overflow-x-auto pb-1 scrollbar-hide w-full sm:w-auto">
                  {!selectedFeedId && (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          variant="outline"
                          size="sm"
                          className="h-9 rounded-xl shadow-sm text-xs font-medium border-border/50 shrink-0"
                        >
                          <Filter className="w-3.5 h-3.5 mr-2" />
                          <span className="truncate max-w-[120px]">
                            {selectedAuthorLabel}
                          </span>
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent
                        align="start"
                        sideOffset={8}
                        className="rounded-xl shadow-md"
                      >
                        <DropdownMenuItem
                          onClick={() => setAuthorFilterId("all")}
                          className="rounded-lg"
                        >
                          {t("rss.allAuthors")}
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        {authorOptions.map((option) => (
                          <DropdownMenuItem
                            key={option.id}
                            onClick={() => setAuthorFilterId(option.id)}
                            className="rounded-lg"
                          >
                            <span className="truncate max-w-[200px]">
                              {option.name}
                            </span>
                            <span className="ml-auto text-xs text-muted-foreground">
                              ({option.count})
                            </span>
                          </DropdownMenuItem>
                        ))}
                      </DropdownMenuContent>
                    </DropdownMenu>
                  )}

                  <Tabs
                    value={videoTypeFilter}
                    onValueChange={(v) =>
                      setVideoTypeFilter(v as "all" | "video" | "short")
                    }
                    className="shrink-0"
                  >
                    <TabsList className="bg-background/80 border border-border/50 rounded-xl h-9 inline-flex p-1 shadow-sm">
                      <TabsTrigger
                        value="all"
                        className="rounded-lg text-xs px-2 sm:px-3 data-[state=active]:shadow-sm gap-1.5"
                      >
                        <Filter className="w-3 h-3" />
                        <span>{t("download.all")}</span>
                        <Badge
                          variant="secondary"
                          className="ml-0.5 h-4 px-1 text-[9px] bg-muted/50"
                        >
                          {typeCounts.all}
                        </Badge>
                      </TabsTrigger>
                      <TabsTrigger
                        value="video"
                        className="rounded-lg text-xs px-2 sm:px-3 data-[state=active]:shadow-sm gap-1.5"
                      >
                        <Video className="w-3.5 h-3.5" />
                        <span className="hidden sm:inline">
                          {t("rss.videos")}
                        </span>
                        <Badge
                          variant="secondary"
                          className="ml-0.5 h-4 px-1 text-[9px] bg-muted/50"
                        >
                          {typeCounts.video}
                        </Badge>
                      </TabsTrigger>
                      <TabsTrigger
                        value="short"
                        className="rounded-lg text-xs px-2 sm:px-3 data-[state=active]:shadow-sm gap-1.5"
                      >
                        <Smartphone className="w-3.5 h-3.5" />
                        <span className="hidden sm:inline">
                          {t("rss.shorts")}
                        </span>
                        <Badge
                          variant="secondary"
                          className="ml-0.5 h-4 px-1 text-[9px] bg-muted/50"
                        >
                          {typeCounts.short}
                        </Badge>
                      </TabsTrigger>
                    </TabsList>
                  </Tabs>
                </div>

                {newTopItems.length > 0 && (
                  <div className="shrink-0">
                    <Button
                      onClick={handleDownloadAllNewTop}
                      variant="default"
                      size="sm"
                      className="h-9 rounded-xl shadow-sm text-xs font-semibold w-full sm:w-auto"
                    >
                      <DownloadCloud className="w-4 h-4 mr-2" />
                      {t("rss.downloadAllNew")} ({newTopItems.length})
                    </Button>
                  </div>
                )}
              </div>

              {/* Content grid */}
              <FeedDetailView
                showAll={!selectedFeedId}
                items={renderedItems}
                totalCount={visibleItems.length}
                onLoadMore={() => setVisibleCount((prev) => prev + 120)}
                onPlay={openPlayer}
              />
            </>
          ) : (
            <EmptyState onAdd={() => setShowAddDialog(true)} />
          )}
        </ScrollArea>

        {/* Add Feed Dialog */}
        <AddFeedDialog
          open={showAddDialog}
          onOpenChange={setShowAddDialog}
          onAdd={addFeed}
        />

        {/* Custom video player — uses yt-dlp for direct streaming */}
        {playerUrl && (
          <VideoPlayer
            url={playerUrl}
            title={playerTitle}
            isShort={playerIsShort}
            onClose={closePlayer}
            onDownload={() => {
              if (playerUrl) {
                commands
                  .startDownload(playerUrl)
                  .then(() => toast.success("Download started"))
                  .catch((err: unknown) =>
                    toast.error(`Download failed: ${err}`),
                  );
              }
            }}
          />
        )}
      </div>
    </TooltipProvider>
  );
}

/* ─── Channel Tab ──────────────────────────────────────── */

function ChannelTab({
  feed,
  isActive,
  syncEntry,
  onClick,
  onRefresh,
  onRemove,
}: {
  feed: RssFeed;
  isActive: boolean;
  syncEntry?: RssSyncProgressEvent;
  onClick: () => void;
  onRefresh: () => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const hasNewItems = feed.items?.some((i) => i.status === "not_queued");
  const itemCount = feed.items?.length || 0;
  const isSyncing =
    syncEntry?.phase === "fetching" || syncEntry?.phase === "importing";
  const progressValue = Math.max(0, Math.min(syncEntry?.progress ?? 0, 100));
  const progressCircumference = 2 * Math.PI * 25;
  const progressOffset =
    progressCircumference - (progressValue / 100) * progressCircumference;

  return (
    <DropdownMenu>
      <div className="relative flex flex-col items-center gap-2 min-w-[64px]">
        <DropdownMenuTrigger asChild>
          <button
            onClick={onClick}
            className={`flex flex-col items-center gap-2 transition-all group ${
              isActive
                ? "opacity-100 scale-100"
                : "opacity-60 hover:opacity-100 scale-95 hover:scale-100"
            }`}
          >
            <div className="relative">
              {isSyncing && (
                <svg
                  className="absolute -inset-1.5 w-[68px] h-[68px] sm:w-[76px] sm:h-[76px] pointer-events-none"
                  viewBox="0 0 76 76"
                  aria-hidden="true"
                >
                  <circle
                    cx="38"
                    cy="38"
                    r="34"
                    fill="none"
                    stroke="hsl(var(--muted-foreground) / 0.2)"
                    strokeWidth="3"
                  />
                  <circle
                    cx="38"
                    cy="38"
                    r="34"
                    fill="none"
                    stroke="hsl(var(--primary))"
                    strokeWidth="3"
                    strokeLinecap="round"
                    strokeDasharray={progressCircumference}
                    strokeDashoffset={progressOffset}
                    transform="rotate(-90 38 38)"
                  />
                </svg>
              )}
              {feed.channelAvatar ? (
                <div className="relative w-14 h-14 sm:w-16 sm:h-16 rounded-full overflow-hidden shadow-sm group-hover:shadow-md transition-shadow">
                  <img
                    src={feed.channelAvatar}
                    alt={feed.channelName || feed.title}
                    className="w-full h-full object-cover"
                  />
                  {isActive && (
                    <div className="absolute inset-0 ring-2 ring-primary ring-inset rounded-full" />
                  )}
                  <div className="absolute inset-0 bg-gradient-to-t from-black/20 to-transparent" />
                </div>
              ) : (
                <div
                  className={`w-14 h-14 sm:w-16 sm:h-16 rounded-full bg-muted/80 flex items-center justify-center text-lg font-bold shadow-sm transition-all ${
                    isActive
                      ? "ring-2 ring-primary ring-inset text-primary bg-primary/10"
                      : "group-hover:bg-muted text-muted-foreground"
                  }`}
                >
                  {(feed.channelName || feed.title || "?")[0]?.toUpperCase()}
                </div>
              )}
              {hasNewItems && (
                <div className="absolute -top-1 -right-1 w-4 h-4 bg-green-500 rounded-full border-2 border-background shadow-sm" />
              )}
            </div>
            <span
              className={`text-[11px] font-semibold truncate w-full text-center px-1 ${isActive ? "text-primary" : "text-foreground"}`}
            >
              {feed.channelName || feed.title || t("rss.channel")}
            </span>
          </button>
        </DropdownMenuTrigger>

        <DropdownMenuContent align="center" sideOffset={4}>
          <DropdownMenuItem disabled className="text-xs text-muted-foreground">
            {t("rss.videosCount", { count: itemCount })}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={onRefresh}>
            <RefreshCw className="w-4 h-4 mr-2" />
            {t("rss.checkNow")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onClick={onRemove}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 className="w-4 h-4 mr-2" />
            {t("rss.remove")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </div>
    </DropdownMenu>
  );
}

/* ─── Feed Detail View ─────────────────────────────────── */

function FeedDetailView({
  showAll,
  items,
  totalCount,
  onLoadMore,
  onPlay,
}: {
  showAll: boolean;
  items: ItemWithFeed[];
  totalCount: number;
  onLoadMore: () => void;
  onPlay: (url: string, title: string, isShort: boolean) => void;
}) {
  const { t } = useTranslation();
  const isShowingShorts =
    items.length > 0 && items.every((item) => inferVideoType(item) === "short");

  if (items.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center">
        <RssIcon className="w-12 h-12 text-muted-foreground/20 mb-4" />
        <p className="text-muted-foreground font-medium">{t("rss.noVideos")}</p>
        <p className="text-sm text-muted-foreground/60 mt-1">
          {showAll ? t("rss.checkFeeds") : t("rss.clickRefresh")}
        </p>
      </div>
    );
  }

  // Shorts get a special compact grid
  if (isShowingShorts) {
    return (
      <div className="pb-6 space-y-4">
        <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-3">
          {items.map((item) => (
            <ShortCard
              key={item.id}
              item={item}
              feedTitle={item.feedTitle}
              feedAvatar={item.feedAvatar}
              onPlay={onPlay}
            />
          ))}
        </div>
        {items.length < totalCount && (
          <div className="flex justify-center">
            <Button variant="outline" onClick={onLoadMore}>
              {t("rss.loadMoreCount", {
                loaded: items.length,
                total: totalCount,
              })}
            </Button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="pb-6 space-y-4">
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 sm:gap-6">
        {items.map((item) => (
          <VideoCard
            key={item.id}
            item={item}
            feedTitle={item.feedTitle}
            feedAvatar={item.feedAvatar}
            onPlay={onPlay}
          />
        ))}
      </div>
      {items.length < totalCount && (
        <div className="flex justify-center pt-4">
          <Button
            variant="outline"
            onClick={onLoadMore}
            className="rounded-xl h-10 px-6 font-medium bg-background/50 backdrop-blur-sm"
          >
            {t("rss.loadMoreCount", {
              loaded: items.length,
              total: totalCount,
            })}
          </Button>
        </div>
      )}
    </div>
  );
}

interface ItemWithFeed extends RssItem {
  feedId: string;
  feedTitle?: string;
  feedAvatar?: string;
}

/* ─── Video Card (regular videos) ──────────────────────── */

const VideoCard = memo(function VideoCard({
  item,
  feedTitle,
  feedAvatar,
  onPlay,
}: {
  item: RssItem;
  feedTitle?: string;
  feedAvatar?: string;
  onPlay: (url: string, title: string, isShort: boolean) => void;
}) {
  const { t } = useTranslation();
  const [downloading, setDownloading] = useState(false);

  const handleDownload = async () => {
    if (!item.url) return;
    setDownloading(true);
    try {
      await commands.startDownload(item.url);
      toast.success(t("download.downloading"));
    } catch (err) {
      toast.error(`Failed: ${err}`);
    } finally {
      setDownloading(false);
    }
  };

  const isShort = inferVideoType(item) === "short";

  const statusBadge = () => {
    switch (item.status) {
      case "downloaded":
        return (
          <Badge className="bg-green-500/10 text-green-500 border-green-500/20 text-[10px] gap-1">
            <CheckCircle2 className="w-2.5 h-2.5" />
            {t("download.completed")}
          </Badge>
        );
      case "queued":
        return (
          <Badge className="bg-blue-500/10 text-blue-500 border-blue-500/20 text-[10px] gap-1">
            <Clock className="w-2.5 h-2.5" />
            {t("download.queued")}
          </Badge>
        );
      default:
        return null;
    }
  };

  return (
    <div className="group rounded-[20px] overflow-hidden border border-border/50 dark:border-white/10 bg-card/60 backdrop-blur-md hover:border-border transition-all hover:shadow-md">
      {/* Thumbnail */}
      <div className="relative aspect-video bg-muted">
        {item.thumbnail ? (
          <img
            src={item.thumbnail}
            alt={item.title}
            className="w-full h-full object-cover"
            loading="lazy"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center">
            <RssIcon className="w-8 h-8 text-muted-foreground/30" />
          </div>
        )}

        {/* Video type badge */}
        {isShort && (
          <div className="absolute top-2 left-2">
            <Badge className="bg-red-500/90 text-white border-0 text-[10px] gap-1 shadow-sm">
              <Smartphone className="w-2.5 h-2.5" />
              Short
            </Badge>
          </div>
        )}

        {/* Overlay actions */}
        <div className="absolute inset-0 bg-black/0 group-hover:bg-black/50 transition-colors flex items-center justify-center opacity-0 group-hover:opacity-100">
          <div className="flex gap-2">
            <Button
              size="icon"
              variant="ghost"
              className="w-10 h-10 rounded-full bg-white/90 flex items-center justify-center shadow-lg hover:bg-white/95"
              onClick={() => onPlay(item.url, item.title, isShort)}
              aria-label={t("player.watch")}
              title={t("player.watch")}
            >
              <Play className="w-4 h-4 text-black" />
            </Button>
            <Button
              size="sm"
              variant="secondary"
              className="h-9 rounded-lg shadow-lg"
              onClick={handleDownload}
              disabled={downloading || item.status === "downloaded"}
            >
              {downloading ? (
                <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />
              ) : (
                <Download className="w-4 h-4 mr-1.5" />
              )}
              {downloading ? "..." : t("download.downloadNow")}
            </Button>
          </div>
        </div>

        {/* Status badge */}
        <div className="absolute top-2 right-2">{statusBadge()}</div>

        {/* Date badge */}
        <div className="absolute bottom-2 right-2">
          <span className="bg-black/60 backdrop-blur-md text-white font-medium text-[10px] px-2 py-1 rounded-md shadow-sm">
            {formatDate(item.publishedAt, t)}
          </span>
        </div>
      </div>

      {/* Info */}
      <div className="p-3 sm:p-4 bg-card/40 backdrop-blur-sm">
        <h3 className="text-sm sm:text-base font-bold line-clamp-2 leading-snug tracking-tight">
          {item.title}
        </h3>
        <div className="flex items-center gap-2 mt-3">
          {feedAvatar ? (
            <img
              src={feedAvatar}
              alt=""
              className="w-5 h-5 sm:w-6 sm:h-6 rounded-full border border-white/10"
            />
          ) : (
            <div className="w-5 h-5 sm:w-6 sm:h-6 rounded-full bg-muted flex items-center justify-center text-[9px] sm:text-[10px] font-bold text-muted-foreground shadow-sm">
              {(feedTitle || "?")[0]?.toUpperCase()}
            </div>
          )}
          <span className="text-xs sm:text-sm font-medium text-muted-foreground truncate flex-1">
            {feedTitle}
          </span>
        </div>
      </div>
    </div>
  );
});

/* ─── Short Card (special layout for Shorts) ──────────── */

const ShortCard = memo(function ShortCard({
  item,
  feedTitle,
  feedAvatar,
  onPlay,
}: {
  item: RssItem;
  feedTitle?: string;
  feedAvatar?: string;
  onPlay: (url: string, title: string, isShort: boolean) => void;
}) {
  const { t } = useTranslation();
  const [downloading, setDownloading] = useState(false);

  const handleDownload = async () => {
    if (!item.url) return;
    setDownloading(true);
    try {
      await commands.startDownload(item.url);
      toast.success(t("download.downloading"));
    } catch (err) {
      toast.error(`Failed: ${err}`);
    } finally {
      setDownloading(false);
    }
  };

  return (
    <div
      className="group relative rounded-[20px] overflow-hidden border border-border/50 dark:border-white/10 bg-card/60 backdrop-blur-md hover:border-border transition-all hover:shadow-md cursor-pointer"
      onClick={() => onPlay(item.url, item.title, true)}
    >
      {/* Thumbnail — 9:16 aspect ratio for Shorts */}
      <div className="relative aspect-[9/16] bg-muted">
        {item.thumbnail ? (
          <img
            src={item.thumbnail}
            alt={item.title}
            className="w-full h-full object-cover"
            loading="lazy"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center bg-gradient-to-b from-muted to-muted/50">
            <Smartphone className="w-8 h-8 text-muted-foreground/30" />
          </div>
        )}

        {/* Short badge */}
        <div className="absolute top-2 left-2">
          <Badge className="bg-red-500/90 text-white border-0 text-[10px] gap-1 shadow-sm">
            <Smartphone className="w-2.5 h-2.5" />
            Short
          </Badge>
        </div>

        {/* Status badge */}
        {item.status === "downloaded" && (
          <div className="absolute top-2 right-2">
            <Badge className="bg-green-500/10 text-green-500 border-green-500/20 text-[10px]">
              <CheckCircle2 className="w-2.5 h-2.5" />
            </Badge>
          </div>
        )}

        {/* Play overlay */}
        <div className="absolute inset-0 bg-black/0 group-hover:bg-black/40 transition-colors flex items-center justify-center opacity-0 group-hover:opacity-100">
          <div className="flex flex-col items-center gap-2">
            <div className="w-14 h-14 rounded-full bg-white/20 backdrop-blur-sm flex items-center justify-center">
              <Play className="w-7 h-7 text-white fill-white ml-0.5" />
            </div>
            <Button
              size="sm"
              variant="secondary"
              className="h-7 text-xs rounded-lg shadow-lg"
              onClick={(e) => {
                e.stopPropagation();
                handleDownload();
              }}
              disabled={downloading || item.status === "downloaded"}
            >
              {downloading ? (
                <Loader2 className="w-3 h-3 animate-spin" />
              ) : (
                <Download className="w-3 h-3 mr-1" />
              )}
              {downloading ? "..." : t("download.downloadNow")}
            </Button>
          </div>
        </div>

        {/* Bottom gradient with title */}
        <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 via-black/40 to-transparent p-3 pt-10">
          <p className="text-white text-xs font-medium line-clamp-2 leading-snug">
            {item.title}
          </p>
          <div className="flex items-center gap-1.5 mt-1.5">
            {feedAvatar ? (
              <img src={feedAvatar} alt="" className="w-4 h-4 rounded-full" />
            ) : (
              <div className="w-4 h-4 rounded-full bg-white/20 flex items-center justify-center text-[7px] font-bold text-white">
                {(feedTitle || "?")[0]?.toUpperCase()}
              </div>
            )}
            <span className="text-white/70 text-[10px] truncate">
              {feedTitle}
            </span>
            <span className="text-white/50 text-[10px] ml-auto">
              {formatDate(item.publishedAt, t)}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
});

/* ─── Add Feed Dialog ──────────────────────────────────── */

function AddFeedDialog({
  open,
  onOpenChange,
  onAdd,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdd: (url: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [adding, setAdding] = useState(false);

  const handleAdd = async () => {
    if (!url.trim()) return;
    setAdding(true);
    try {
      await onAdd(url.trim());
      setUrl("");
      onOpenChange(false);
    } finally {
      setAdding(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("rss.addFeed")}</DialogTitle>
          <DialogDescription>{t("rss.subtitle")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="space-y-2">
            <Label>{t("rss.feedUrl")}</Label>
            <Input
              placeholder={t("rss.feedUrlPlaceholder")}
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleAdd()}
              autoFocus
            />
            <p className="text-xs text-muted-foreground">
              {t("rss.feedUrlHint")}
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button onClick={handleAdd} disabled={!url.trim() || adding}>
            {adding && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
            {t("rss.addFeed")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/* ─── Empty State ──────────────────────────────────────── */

function EmptyState({ onAdd }: { onAdd: () => void }) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <div className="w-20 h-20 rounded-full bg-muted/50 flex items-center justify-center mb-6">
        <RssIcon className="w-10 h-10 text-muted-foreground/30" />
      </div>
      <p className="text-lg font-medium text-muted-foreground mb-1">
        {t("rss.noFeeds")}
      </p>
      <p className="text-sm text-muted-foreground/60 mb-6 max-w-[300px]">
        {t("rss.noFeedsDesc")}
      </p>
      <Button onClick={onAdd} className="rounded-lg">
        <Plus className="w-4 h-4 mr-2" />
        {t("rss.addFeed")}
      </Button>
    </div>
  );
}

/* ─── Helpers ──────────────────────────────────────────── */

function formatDate(
  dateStr: string,
  t?: (key: string, opts?: Record<string, unknown>) => string,
): string {
  if (!dateStr) return "";
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffH = Math.floor(diffMs / 3600000);
    if (diffH < 1) return t ? t("rss.justNow") : "Just now";
    if (diffH < 24)
      return t ? t("rss.hoursAgo", { count: diffH }) : `${diffH}h ago`;
    const diffD = Math.floor(diffH / 24);
    if (diffD < 7)
      return t ? t("rss.daysAgo", { count: diffD }) : `${diffD}d ago`;
    if (diffD < 30)
      return t
        ? t("rss.weeksAgo", { count: Math.floor(diffD / 7) })
        : `${Math.floor(diffD / 7)}w ago`;
    return date.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: date.getFullYear() !== now.getFullYear() ? "numeric" : undefined,
    });
  } catch {
    return dateStr;
  }
}

function inferVideoType(item: RssItem): "video" | "short" {
  if (item.videoType === "short") return "short";
  if (item.videoType === "video") return "video";

  const url = (item.url || "").toLowerCase();
  const title = (item.title || "").toLowerCase();

  if (url.includes("/shorts/")) return "short";
  if (title.includes("#short") || title.includes("#shorts")) return "short";

  return "video";
}
