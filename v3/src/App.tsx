import { useState, useEffect } from "react";
import { HashRouter, Routes, Route } from "react-router-dom";
import { Provider as JotaiProvider } from "jotai";
import { ThemeProvider } from "@/components/ThemeProvider";
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
import "./i18n";

export default function App() {
  const [setupDone, setSetupDone] = useState<boolean | null>(null);

  useEffect(() => {
    // Check if required components are available
    const checkSetup = async () => {
      try {
        const [ytdlp, ffmpeg] = await Promise.all([
          commands.checkYtdlp(),
          commands.checkFfmpeg(),
        ]);
        setSetupDone(ytdlp && ffmpeg);
      } catch {
        setSetupDone(false);
      }
    };
    checkSetup();
  }, []);

  return (
    <JotaiProvider>
      <ThemeProvider>
        <HashRouter>
          <div className="app-root flex h-screen w-screen overflow-hidden bg-background text-foreground">
            {setupDone === false ? (
              /* Setup wizard when components missing */
              <div className="flex-1 flex flex-col overflow-hidden">
                <TitleBar />
                <SetupPage onComplete={() => setSetupDone(true)} />
              </div>
            ) : (
              <>
                {/* Sidebar navigation (Desktop) */}
                <Sidebar />

                {/* Mobile navigation (Bottom) */}
                <MobileNav />

                {/* Main content area with floating titlebar */}
                <div className="flex-1 flex flex-col overflow-hidden relative mb-[60px] md:mb-0">
                  {/* Floating title bar buttons - positioned over content */}
                  <TitleBar />

                  {/* Page content */}
                  <main className="flex-1 overflow-y-auto">
                    <Routes>
                      <Route path="/" element={<DashboardPage />} />
                      <Route path="/download" element={<DownloadPage />} />
                      <Route path="/rss" element={<RssPage />} />
                      <Route path="/transcribe" element={<TranscribePage />} />
                      <Route
                        path="/supported"
                        element={<SupportedSitesPage />}
                      />
                      <Route path="/settings" element={<SettingsPage />} />
                      <Route path="/about" element={<AboutPage />} />
                    </Routes>
                  </main>
                </div>

                {/* Update checker - shows on startup if updates available */}
                {setupDone && <UpdateChecker />}
              </>
            )}
          </div>
        </HashRouter>
        <Toaster position="bottom-right" richColors closeButton />
      </ThemeProvider>
    </JotaiProvider>
  );
}
