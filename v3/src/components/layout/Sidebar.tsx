import { cn } from "@/lib/utils";
import { useTranslation } from "react-i18next";
import { useNavigate, useLocation } from "react-router-dom";
import { Download, Rss, Mic, Globe, Settings, Info, Home } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
} from "@/components/ui/tooltip";

const navItems = [
  { path: "/download", icon: Download, labelKey: "nav.download" },
  { path: "/rss", icon: Rss, labelKey: "nav.rss" },
  { path: "/transcribe", icon: Mic, labelKey: "nav.transcribe" },
  { path: "/supported", icon: Globe, labelKey: "nav.supportedSites" },
];

const bottomItems = [
  { path: "/settings", icon: Settings, labelKey: "nav.settings" },
  { path: "/about", icon: Info, labelKey: "nav.about" },
];

export function Sidebar() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const currentPath = location.pathname;

  return (
    <TooltipProvider delayDuration={0}>
      <aside className="titlebar-drag relative z-40 hidden md:flex flex-col items-center w-[88px] min-w-[88px] py-3 px-2">
        {/* Floating pill container */}
        <div className="titlebar-no-drag flex flex-col items-center w-full h-full glass-sidebar rounded-2xl shadow-lg py-3 gap-0.5">
          {/* Logo at top */}
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => navigate("/")}
                className={cn(
                  "flex items-center justify-center w-12 h-12 rounded-xl transition-all duration-200 mb-1",
                  currentPath === "/"
                    ? "bg-primary text-primary-foreground shadow-lg shadow-primary/30"
                    : "text-sidebar-foreground/50 hover:text-sidebar-foreground hover:bg-sidebar-foreground/8",
                )}
              >
                <Home className="w-6 h-6" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">{t("nav.home")}</TooltipContent>
          </Tooltip>

          {/* Divider */}
          <div className="w-7 h-px bg-border/60 my-1.5" />

          {/* Main navigation */}
          <nav className="flex flex-col items-center gap-0.5 flex-1">
            {navItems.map(({ path, icon: Icon, labelKey }) => (
              <Tooltip key={path}>
                <TooltipTrigger asChild>
                  <button
                    onClick={() => navigate(path)}
                    className={cn(
                      "flex items-center justify-center w-12 h-12 rounded-xl transition-all duration-200",
                      currentPath === path
                        ? "bg-primary text-primary-foreground shadow-lg shadow-primary/30"
                        : "text-sidebar-foreground/50 hover:text-sidebar-foreground hover:bg-sidebar-foreground/8",
                    )}
                  >
                    <Icon className="w-6 h-6" />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="right">{t(labelKey)}</TooltipContent>
              </Tooltip>
            ))}
          </nav>

          {/* Divider */}
          <div className="w-7 h-px bg-border/60 my-1.5" />

          {/* Bottom nav */}
          <nav className="flex flex-col items-center gap-0.5">
            {bottomItems.map(({ path, icon: Icon, labelKey }) => (
              <Tooltip key={path}>
                <TooltipTrigger asChild>
                  <button
                    onClick={() => navigate(path)}
                    className={cn(
                      "flex items-center justify-center w-12 h-12 rounded-xl transition-all duration-200",
                      currentPath === path
                        ? "bg-primary text-primary-foreground shadow-lg shadow-primary/30"
                        : "text-sidebar-foreground/50 hover:text-sidebar-foreground hover:bg-sidebar-foreground/8",
                    )}
                  >
                    <Icon className="w-6 h-6" />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="right">{t(labelKey)}</TooltipContent>
              </Tooltip>
            ))}
          </nav>
        </div>
      </aside>
    </TooltipProvider>
  );
}
