import { useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card } from "@/components/ui/card";
import {
  Loader2,
  AlertCircle,
  List,
  ClipboardPaste,
  ChevronDown,
} from "lucide-react";
import { commands, type PlaylistInfo } from "@/lib/tauri";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

interface PlaylistDownloadProps {
  onDownloadStart: () => void;
}

export function PlaylistDownload({ onDownloadStart }: PlaylistDownloadProps) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [playlist, setPlaylist] = useState<PlaylistInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [startIndex, setStartIndex] = useState("1");
  const [endIndex, setEndIndex] = useState("");
  const [selectedQuality, setSelectedQuality] = useState<string>("best");

  const handleFetchPlaylist = useCallback(async () => {
    if (!url.trim()) return;

    setLoading(true);
    setError(null);
    setPlaylist(null);

    try {
      const info = await commands.getPlaylistInfo(url.trim());
      setPlaylist(info);
      setSelectedIds(new Set());
      setStartIndex("1");
      setEndIndex(String(info.entryCount));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch playlist");
    } finally {
      setLoading(false);
    }
  }, [url]);

  const handleDownloadPlaylist = useCallback(async () => {
    if (!playlist) return;

    const start = parseInt(startIndex, 10) || 1;
    const end = parseInt(endIndex, 10) || playlist.entryCount;

    try {
      await commands.startPlaylistDownload({
        url: url.trim(),
        startIndex: start,
        endIndex: end,
        format: selectedQuality,
      });
      onDownloadStart();
      setUrl("");
      setPlaylist(null);
      setSelectedIds(new Set());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to start download");
    }
  }, [playlist, url, startIndex, endIndex, selectedQuality, onDownloadStart]);

  const handlePaste = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      setUrl(text.trim());
    } catch (err) {
      console.error("Failed to paste:", err);
    }
  }, []);

  return (
    <div className="flex flex-col h-full gap-4">
      {/* URL Input */}
      <div className="flex gap-2 mb-4">
        <Input
          placeholder="Enter playlist URL..."
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleFetchPlaylist()}
          className="flex-1 h-11"
        />
        <Button variant="outline" onClick={handlePaste} className="h-11">
          <ClipboardPaste className="w-4 h-4 mr-2" />
          Paste
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" className="h-11">
              <ChevronDown className="w-4 h-4 mr-2" />
              {selectedQuality === "best"
                ? "Best"
                : selectedQuality === "2160p"
                  ? "4K (2160p)"
                  : selectedQuality === "1440p"
                    ? "1440p"
                    : selectedQuality === "1080p"
                      ? "1080p"
                      : selectedQuality === "720p"
                        ? "720p"
                        : selectedQuality === "480p"
                          ? "480p"
                          : "Audio only"}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => setSelectedQuality("best")}>
              Best quality
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSelectedQuality("2160p")}>
              4K (2160p)
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSelectedQuality("1440p")}>
              1440p
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSelectedQuality("1080p")}>
              1080p
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSelectedQuality("720p")}>
              720p
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSelectedQuality("480p")}>
              480p
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSelectedQuality("bestaudio")}>
              Audio only
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <Button
          onClick={handleFetchPlaylist}
          disabled={!url.trim() || loading}
          className="h-11"
        >
          {loading ? <Loader2 className="w-4 h-4 animate-spin mr-2" /> : null}
          {t("download.fetchPlaylist")}
        </Button>
      </div>

      {/* Error */}
      {error && (
        <Card className="border-destructive/30 bg-destructive/5 p-4">
          <div className="flex items-start gap-2">
            <AlertCircle className="w-5 h-5 text-destructive shrink-0 mt-0.5" />
            <div>
              <p className="font-medium text-destructive">
                {t("download.error")}
              </p>
              <p className="text-sm text-muted-foreground">{error}</p>
            </div>
          </div>
        </Card>
      )}

      {/* Loading */}
      {loading && (
        <Card className="p-8">
          <div className="flex flex-col items-center gap-3">
            <Loader2 className="w-8 h-8 animate-spin text-primary" />
            <p className="text-sm text-muted-foreground">
              {t("download.fetchingPlaylist")}
            </p>
          </div>
        </Card>
      )}

      {/* Playlist Info */}
      {playlist && !loading && (
        <div className="space-y-4">
          <Card className="p-4">
            <div className="flex items-center gap-3 mb-4">
              <List className="w-5 h-5 text-primary" />
              <div>
                <h3 className="font-semibold">{playlist.title}</h3>
                <p className="text-sm text-muted-foreground">
                  {playlist.entryCount} videos
                </p>
              </div>
            </div>

            {/* Range Selection */}
            <div className="flex gap-2 mb-4">
              <div className="flex-1">
                <label className="text-sm text-muted-foreground mb-1 block">
                  Start
                </label>
                <Input
                  type="number"
                  min="1"
                  max={playlist.entryCount}
                  value={startIndex}
                  onChange={(e) => setStartIndex(e.target.value)}
                  className="h-9"
                />
              </div>
              <div className="flex-1">
                <label className="text-sm text-muted-foreground mb-1 block">
                  End
                </label>
                <Input
                  type="number"
                  min="1"
                  max={playlist.entryCount}
                  value={endIndex}
                  onChange={(e) => setEndIndex(e.target.value)}
                  className="h-9"
                />
              </div>
            </div>

            <Button onClick={handleDownloadPlaylist} className="w-full">
              {t("download.downloadSelected")} (
              {selectedIds.size > 0
                ? selectedIds.size
                : (parseInt(endIndex) || playlist.entryCount) -
                  (parseInt(startIndex) || 1) +
                  1}
              )
            </Button>
          </Card>

        </div>
      )}
    </div>
  );
}
