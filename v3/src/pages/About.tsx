import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ExternalLink, Heart, Github, RefreshCw } from "lucide-react";
import { commands } from "@/lib/tauri";

const CREDITS = [
  { name: "Tauri", url: "https://tauri.app", roleKey: "framework" },
  {
    name: "yt-dlp",
    url: "https://github.com/yt-dlp/yt-dlp",
    roleKey: "downloadEngine",
  },
  { name: "FFmpeg", url: "https://ffmpeg.org", roleKey: "mediaProcessing" },
  {
    name: "whisper.cpp",
    url: "https://github.com/ggerganov/whisper.cpp",
    roleKey: "transcription",
  },
  { name: "React", url: "https://react.dev", roleKey: "uiLibrary" },
  { name: "shadcn/ui", url: "https://ui.shadcn.com", roleKey: "uiComponents" },
  { name: "Tailwind CSS", url: "https://tailwindcss.com", roleKey: "styling" },
];

export function AboutPage() {
  const { t } = useTranslation();
  const [version, setVersion] = useState("3.1.0");

  useEffect(() => {
    commands
      .getAppVersion()
      .then(setVersion)
      .catch(() => {});
  }, []);

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
                {t("about.currentVersion")}: {version}
              </p>
              <p className="text-xs text-muted-foreground">
                {t("about.upToDate")}
              </p>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                handleOpenUrl("https://github.com/diorhc/YTDL/releases/latest")
              }
            >
              <RefreshCw className="w-4 h-4 mr-1.5" />
              {t("about.checkUpdate")}
            </Button>
          </CardContent>
        </Card>

        {/* Credits */}
        <Card>
          <CardContent className="p-0">
            <div className="p-4">
              <p className="text-sm font-medium mb-3">
                {t("about.credits.title")}
              </p>
              <div className="space-y-2">
                {CREDITS.map((c, i) => (
                  <div key={c.name}>
                    <div className="flex items-center justify-between py-1">
                      <div>
                        <p className="text-sm font-medium">{c.name}</p>
                        <p className="text-xs text-muted-foreground">
                          {t(`about.credits.${c.roleKey}`)}
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
