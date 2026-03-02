import { useState, useEffect } from "react";
import { HashRouter, Routes, Route, Navigate } from "react-router-dom";
import { Provider as JotaiProvider, useSetAtom } from "jotai";
import { ThemeProvider } from "@/components/ThemeProvider";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { Toaster } from "@/components/ui/sonner";
import { Sidebar } from "@/components/layout/Sidebar";
import { MobileNav } from "@/components/layout/MobileNav";
import { TitleBar } from "@/components/layout/Titlebar";
import { UpdateChecker } from "@/components/UpdateChecker";
import { DownloadPage } from "@/pages/Download";
import { DashboardPage } from "@/pages/Dashboard";
import { RssPage } from "@/pages/RSS";
import { TranscribePage } from "@/pages/Transcribe";
import { SupportedSitesPage } from "@/pages/SupportedSites";
import { SettingsPage } from "@/pages/Settings";
import { AboutPage } from "@/pages/About";
import { SetupPage } from "@/pages/Setup";
import { commands } from "@/lib/tauri";
import { platformAtom } from "@/store/atoms";

/** Syncs the detected platform value into the Jotai atom so all pages can read it. */
function PlatformSyncer({ platform }: { platform: string }) {
  const setPlatform = useSetAtom(platformAtom);
  useEffect(() => {
    if (platform) setPlatform(platform);
  }, [platform, setPlatform]);
  return null;
}

export default function App() {
  const [setupDone, setSetupDone] = useState<boolean | null>(null);
  const [platform, setPlatform] = useState<string>("");
  const showDesktopTitleBar = platform !== "android" && platform !== "ios";

  useEffect(() => {
    // Check if required components are available.
    // Each check is independent so one failure doesn't block the other.
    const checkSetup = async () => {
      try {
        const [ytdlp, ffmpeg] = await Promise.all([
          commands.checkYtdlp().catch(() => false),
          commands.checkFfmpeg().catch(() => false),
        ]);
        setSetupDone(ytdlp && ffmpeg);
      } catch {
        setSetupDone(false);
      }
    };
    checkSetup();

    commands
      .getPlatform()
      .then(setPlatform)
      .catch(() => setPlatform(""));
  }, []);

  return (
    <JotaiProvider>
      <PlatformSyncer platform={platform} />
      <ErrorBoundary>
        <ThemeProvider>
          <HashRouter>
            <div className="flex h-[100dvh] w-full overflow-hidden bg-background text-foreground sm:rounded-[18px] sm:shadow-[0_12px_40px_rgba(0,0,0,0.6)]">
              {setupDone === null ? (
                /* Loading state while checking setup */
                <div className="flex-1 flex items-center justify-center">
                  <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
                </div>
              ) : setupDone === false ? (
                /* Setup wizard when components missing */
                <div className="flex-1 flex flex-col overflow-hidden">
                  {showDesktopTitleBar && <TitleBar />}
                  <SetupPage onComplete={() => setSetupDone(true)} />
                </div>
              ) : (
                <>
                  {/* Sidebar navigation (Desktop) */}
                  <Sidebar />

                  {/* Mobile navigation (Bottom) */}
                  <MobileNav />

                  {/* Main content area with floating titlebar */}
                  <div className="flex-1 flex flex-col overflow-hidden relative pb-[calc(4rem+env(safe-area-inset-bottom))] md:pb-0">
                    {/* Floating title bar buttons - positioned over content */}
                    {showDesktopTitleBar && <TitleBar />}

                    {/* Page content */}
                    <main className="flex-1 overflow-y-auto overflow-x-hidden pt-[env(safe-area-inset-top)] pl-[env(safe-area-inset-left)] pr-[env(safe-area-inset-right)]">
                      <Routes>
                        <Route path="/" element={<DashboardPage />} />
                        <Route path="/download" element={<DownloadPage />} />
                        <Route path="/rss" element={<RssPage />} />
                        <Route
                          path="/transcribe"
                          element={<TranscribePage />}
                        />
                        <Route
                          path="/supported"
                          element={<SupportedSitesPage />}
                        />
                        <Route path="/settings" element={<SettingsPage />} />
                        <Route path="/about" element={<AboutPage />} />
                        <Route path="*" element={<Navigate to="/" replace />} />
                      </Routes>
                    </main>
                  </div>

                  {/* Update checker - shows on startup if updates available */}
                  {setupDone && <UpdateChecker />}
                </>
              )}
            </div>
          </HashRouter>
          <Toaster position="top-center" richColors closeButton />
        </ThemeProvider>
      </ErrorBoundary>
    </JotaiProvider>
  );
}
