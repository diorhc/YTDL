import { useAtom } from "jotai";
import { useCallback, useEffect, useRef } from "react";
import { commands, events } from "@/lib/tauri";
import { downloadsAtom, downloadLoadingAtom } from "@/store/atoms";
import { toast } from "sonner";

export function useDownloads() {
  const [downloads, setDownloads] = useAtom(downloadsAtom);
  const [loading, setLoading] = useAtom(downloadLoadingAtom);
  const initialized = useRef(false);

  const loadDownloads = useCallback(async () => {
    try {
      setLoading(true);
      const data = await commands.getDownloads();
      setDownloads(data);
    } catch (err) {
      console.error("Failed to load downloads:", err);
    } finally {
      setLoading(false);
    }
  }, [setDownloads, setLoading]);

  // Load downloads from database on mount
  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    loadDownloads();
  }, [loadDownloads]);

  // Listen for download events
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    events
      .onDownloadProgress((evt) => {
        setDownloads((prev) =>
          prev.map((d) =>
            d.id === evt.id
              ? {
                  ...d,
                  progress: evt.progress,
                  speed: evt.speed,
                  eta: evt.eta,
                  status: "downloading",
                }
              : d,
          ),
        );
      })
      .then((fn) => unlisteners.push(fn));

    events
      .onDownloadComplete((evt) => {
        setDownloads((prev) =>
          prev.map((d) =>
            d.id === evt.id
              ? {
                  ...d,
                  status: "completed",
                  progress: 100,
                  filePath: evt.outputPath,
                }
              : d,
          ),
        );
        toast.success("Download completed!");
      })
      .then((fn) => unlisteners.push(fn));

    events
      .onDownloadError((evt) => {
        setDownloads((prev) =>
          prev.map((d) =>
            d.id === evt.id ? { ...d, status: "error", error: evt.error } : d,
          ),
        );
        toast.error(`Download failed: ${evt.error}`);
      })
      .then((fn) => unlisteners.push(fn));

    return () => unlisteners.forEach((fn) => fn());
  }, [setDownloads]);

  const startDownload = useCallback(
    async (url: string, formatId?: string) => {
      try {
        const id = await commands.startDownload(url, formatId);
        // Reload from DB to get real title/thumbnail
        await loadDownloads();
        return id;
      } catch (err) {
        toast.error(`Failed to start download: ${err}`);
        throw err;
      }
    },
    [loadDownloads],
  );

  const pauseDownload = useCallback(
    async (id: string) => {
      try {
        await commands.pauseDownload(id);
        setDownloads((prev) =>
          prev.map((d) => (d.id === id ? { ...d, status: "paused" } : d)),
        );
      } catch (err) {
        toast.error(`Failed to pause: ${err}`);
      }
    },
    [setDownloads],
  );

  const resumeDownload = useCallback(
    async (id: string) => {
      try {
        await commands.resumeDownload(id);
        setDownloads((prev) =>
          prev.map((d) => (d.id === id ? { ...d, status: "downloading" } : d)),
        );
      } catch (err) {
        toast.error(`Failed to resume: ${err}`);
      }
    },
    [setDownloads],
  );

  const cancelDownload = useCallback(
    async (id: string) => {
      try {
        await commands.cancelDownload(id);
        setDownloads((prev) =>
          prev.map((d) => (d.id === id ? { ...d, status: "cancelled" } : d)),
        );
      } catch (err) {
        toast.error(`Failed to cancel: ${err}`);
      }
    },
    [setDownloads],
  );

  const retryDownload = useCallback(
    async (id: string) => {
      try {
        await commands.retryDownload(id);
        setDownloads((prev) =>
          prev.map((d) =>
            d.id === id
              ? { ...d, status: "downloading", progress: 0, error: undefined }
              : d,
          ),
        );
      } catch (err) {
        toast.error(`Failed to retry: ${err}`);
      }
    },
    [setDownloads],
  );

  const deleteDownload = useCallback(
    async (id: string, deleteFile = false) => {
      try {
        await commands.deleteDownload(id, deleteFile);
        setDownloads((prev) => prev.filter((d) => d.id !== id));
      } catch (err) {
        toast.error(`Failed to delete: ${err}`);
      }
    },
    [setDownloads],
  );

  const getVideoInfo = useCallback(async (url: string) => {
    return commands.getVideoInfo(url);
  }, []);

  return {
    downloads,
    loading,
    loadDownloads,
    startDownload,
    pauseDownload,
    resumeDownload,
    cancelDownload,
    retryDownload,
    deleteDownload,
    getVideoInfo,
  };
}
