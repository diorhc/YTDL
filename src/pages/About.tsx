import { useTranslation } from "react-i18next";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ExternalLink, Heart, Github, RefreshCw } from "lucide-react";
import { commands } from "@/lib/tauri";

const CREDITS = [
  { name: "Tauri", url: "https://tauri.app", role: "Framework" },
  {
    name: "yt-dlp",
    url: "https://github.com/yt-dlp/yt-dlp",
    role: "Download engine",
  },
  { name: "FFmpeg", url: "https://ffmpeg.org", role: "Media processing" },
  {
    name: "whisper.cpp",
    url: "https://github.com/ggerganov/whisper.cpp",
    role: "Transcription",
  },
  { name: "React", url: "https://react.dev", role: "UI library" },
  { name: "shadcn/ui", url: "https://ui.shadcn.com", role: "UI components" },
  { name: "Tailwind CSS", url: "https://tailwindcss.com", role: "Styling" },
];

export function AboutPage() {
  const { t } = useTranslation();

  const handleOpenUrl = async (url: string) => {
    try {
      await commands.openExternal(url);
    } catch {
      window.open(url, "_blank");
    }
  };

  return (
    <div className="flex flex-col h-full p-6 items-center">
      <div className="max-w-lg w-full space-y-6 py-8">
        {/* App identity */}
        <div className="flex flex-col items-center text-center space-y-3">
          <div>
            <h1 className="text-3xl font-bold tracking-tight">YTDL</h1>
            <p className="text-muted-foreground text-sm mt-1">
              {t("about.subtitle")}
            </p>
          </div>
        </div>

        {/* Update check */}
        <Card>
          <CardContent className="p-4 flex items-center justify-between">
            <div>
              <p className="text-sm font-medium">
                {t("about.currentVersion")}: 3.0.0
              </p>
              <p className="text-xs text-muted-foreground">
                {t("about.upToDate")}
              </p>
            </div>
            <Button variant="outline" size="sm">
              <RefreshCw className="w-4 h-4 mr-1.5" />
              {t("about.checkUpdate")}
            </Button>
          </CardContent>
        </Card>

        {/* Credits */}
        <Card>
          <CardContent className="p-0">
            <div className="p-4">
              <p className="text-sm font-medium mb-3">{t("about.credits")}</p>
              <div className="space-y-2">
                {CREDITS.map((c, i) => (
                  <div key={c.name}>
                    <div className="flex items-center justify-between py-1">
                      <div>
                        <p className="text-sm font-medium">{c.name}</p>
                        <p className="text-xs text-muted-foreground">
                          {c.role}
                        </p>
                      </div>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => handleOpenUrl(c.url)}
                      >
                        <ExternalLink className="w-3.5 h-3.5" />
                      </Button>
                    </div>
                    {i < CREDITS.length - 1 && <Separator />}
                  </div>
                ))}
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Links */}
        <div className="flex justify-center gap-3">
          <Button
            variant="outline"
            size="sm"
            onClick={() => handleOpenUrl("https://github.com/diorhc/YTDL")}
          >
            <Github className="w-4 h-4 mr-1.5" />
            GitHub
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => handleOpenUrl("https://github.com/sponsors/diorhc")}
          >
            <Heart className="w-4 h-4 mr-1.5" />
            {t("about.sponsor")}
          </Button>
        </div>

        {/* Footer */}
        <p className="text-center text-xs text-muted-foreground">
          {t("about.license")}
        </p>
      </div>
    </div>
  );
}
