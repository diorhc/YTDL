import { useAtom } from "jotai";
import { useCallback, useEffect, useRef } from "react";
import { commands, type RssFeed } from "@/lib/tauri";
import { feedsAtom, feedsLoadingAtom } from "@/store/atoms";
import { toast } from "sonner";

export function useRss() {
  const [feeds, setFeeds] = useAtom(feedsAtom);
  const [loading, setLoading] = useAtom(feedsLoadingAtom);
  const initialized = useRef(false);

  const loadFeeds = useCallback(async () => {
    try {
      setLoading(true);
      const data = await commands.getFeeds();
      setFeeds(data as unknown as RssFeed[]);
    } catch (err) {
      console.error("Failed to load feeds:", err);
    } finally {
      setLoading(false);
    }
  }, [setFeeds, setLoading]);

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    loadFeeds();
  }, [loadFeeds]);

  const addFeed = useCallback(
    async (url: string) => {
      try {
        const feedId = await commands.addFeed(url);
        // Add feed to UI immediately with placeholder
        setFeeds((prev) => [
          {
            id: feedId,
            url,
            title: url,
            channelName: "",
            channelAvatar: "",
            lastChecked: "",
            autoDownload: false,
            keywords: [],
            ignoreKeywords: [],
            items: [],
          } as RssFeed,
          ...prev,
        ]);
        toast.success("Feed added successfully");

        // Run initial sync in background so adding a feed feels instant.
        void (async () => {
          try {
            const items = await commands.checkFeed(feedId);
            setFeeds((prev) =>
              prev.map((f) =>
                f.id === feedId
                  ? {
                      ...f,
                      items: items as unknown as RssFeed["items"],
                      lastChecked: new Date().toISOString(),
                    }
                  : f,
              ),
            );
            toast.success(`Feed synced: ${items.length} items`);
          } catch (err) {
            toast.error(`Feed added, but sync failed: ${err}`);
          } finally {
            // Refresh metadata (title/avatar/status) from DB.
            void loadFeeds();
          }
        })();
      } catch (err) {
        toast.error(`Failed to add feed: ${err}`);
      }
    },
    [setFeeds, loadFeeds],
  );

  const removeFeed = useCallback(
    async (id: string) => {
      try {
        await commands.removeFeed(id);
        setFeeds((prev) => prev.filter((f) => f.id !== id));
        toast.success("Feed removed");
      } catch (err) {
        toast.error(`Failed to remove feed: ${err}`);
      }
    },
    [setFeeds],
  );

  const checkFeed = useCallback(
    async (id: string) => {
      try {
        toast.info("Checking feed...");
        const items = await commands.checkFeed(id);
        setFeeds((prev) =>
          prev.map((f) =>
            f.id === id
              ? {
                  ...f,
                  items: items as unknown as RssFeed["items"],
                  lastChecked: new Date().toISOString(),
                }
              : f,
          ),
        );

        // Keep DB and UI in sync in background.
        void loadFeeds();
        toast.success(`Feed updated: ${items.length} items`);
      } catch (err) {
        toast.error(`Failed to check feed: ${err}`);
      }
    },
    [setFeeds, loadFeeds],
  );

  return { feeds, loading, loadFeeds, addFeed, removeFeed, checkFeed };
}
