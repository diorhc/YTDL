import { useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Search, Globe } from "lucide-react";

const popularSites = [
  { name: "YouTube", domain: "youtube.com", icon: "🎬" },
  { name: "YouTube Music", domain: "music.youtube.com", icon: "🎵" },
  { name: "TikTok", domain: "tiktok.com", icon: "📱" },
  { name: "Instagram", domain: "instagram.com", icon: "📸" },
  { name: "Twitter/X", domain: "twitter.com", icon: "🐦" },
  { name: "Facebook", domain: "facebook.com", icon: "👤" },
  { name: "Twitch", domain: "twitch.tv", icon: "🎮" },
  { name: "Vimeo", domain: "vimeo.com", icon: "🎥" },
  { name: "Dailymotion", domain: "dailymotion.com", icon: "📺" },
  { name: "SoundCloud", domain: "soundcloud.com", icon: "🎧" },
  { name: "Bilibili", domain: "bilibili.com", icon: "📹" },
  { name: "Reddit", domain: "reddit.com", icon: "🔵" },
  { name: "VK", domain: "vk.com", icon: "🔷" },
  { name: "Rutube", domain: "rutube.ru", icon: "🟢" },
  { name: "Odnoklassniki", domain: "ok.ru", icon: "🟠" },
  { name: "Pornhub", domain: "pornhub.com", icon: "🔞" },
  { name: "Bandcamp", domain: "bandcamp.com", icon: "🎶" },
  { name: "Spotify", domain: "spotify.com", icon: "🟢" },
  { name: "Apple Music", domain: "music.apple.com", icon: "🍎" },
  { name: "Yandex Music", domain: "music.yandex.ru", icon: "🎵" },
  { name: "Crunchyroll", domain: "crunchyroll.com", icon: "🍙" },
  { name: "Niconico", domain: "nicovideo.jp", icon: "📺" },
  { name: "LinkedIn", domain: "linkedin.com", icon: "💼" },
  { name: "Tumblr", domain: "tumblr.com", icon: "📝" },
  { name: "Ted", domain: "ted.com", icon: "🎤" },
  { name: "ESPN", domain: "espn.com", icon: "⚽" },
  { name: "NBC", domain: "nbc.com", icon: "📺" },
  { name: "Arte", domain: "arte.tv", icon: "🎨" },
  { name: "Disney+", domain: "disneyplus.com", icon: "✨" },
  { name: "Rumble", domain: "rumble.com", icon: "📣" },
];

export function SupportedSitesPage() {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");

  const filteredSites = useMemo(() => {
    if (!search.trim()) return popularSites;
    const q = search.toLowerCase();
    return popularSites.filter(
      (s) =>
        s.name.toLowerCase().includes(q) || s.domain.toLowerCase().includes(q),
    );
  }, [search]);

  return (
    <div className="flex flex-col h-full p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-bold">{t("supported.title")}</h1>
        <p className="text-sm text-muted-foreground mt-1">
          {t("supported.subtitle")}
        </p>
      </div>

      {/* Search */}
      <div className="relative mb-4">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
        <Input
          placeholder={t("supported.search")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="pl-9"
        />
      </div>

      <p className="text-sm text-muted-foreground mb-4">
        {t("supported.popular")}
      </p>

      <ScrollArea className="flex-1">
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3">
          {filteredSites.map((site) => (
            <Card
              key={site.domain}
              className="hover:bg-accent/5 transition-colors cursor-pointer"
            >
              <CardContent className="p-4 flex items-center gap-3">
                <span className="text-2xl">{site.icon}</span>
                <div className="min-w-0">
                  <p className="font-medium text-sm truncate">{site.name}</p>
                  <p className="text-xs text-muted-foreground truncate">
                    {site.domain}
                  </p>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>

        {filteredSites.length === 0 && (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <Globe className="w-12 h-12 text-muted-foreground/30 mb-4" />
            <p className="text-muted-foreground">{t("common.noResults")}</p>
          </div>
        )}

        <div className="mt-6 text-center text-sm text-muted-foreground">
          <Badge variant="outline">1000+</Badge> sites supported via yt-dlp
        </div>
      </ScrollArea>
    </div>
  );
}
