import { cn } from "@/lib/utils";
import { useTranslation } from "react-i18next";
import { useNavigate, useLocation } from "react-router-dom";
import { Download, Rss, Mic, Settings, Home } from "lucide-react";

const navItems = [
    { path: "/", icon: Home, labelKey: "nav.home" },
    { path: "/download", icon: Download, labelKey: "nav.download" },
    { path: "/rss", icon: Rss, labelKey: "nav.rss" },
    { path: "/transcribe", icon: Mic, labelKey: "nav.transcribe" },
    { path: "/settings", icon: Settings, labelKey: "nav.settings" },
];

export function MobileNav() {
    const { t } = useTranslation();
    const navigate = useNavigate();
    const location = useLocation();
    const currentPath = location.pathname;

    return (
        <nav className="md:hidden fixed bottom-0 left-0 right-0 z-50 bg-background/80 backdrop-blur-lg border-t pb-[env(safe-area-inset-bottom)]">
            <div className="flex items-center justify-around p-2">
                {navItems.map(({ path, icon: Icon, labelKey }) => (
                    <button
                        key={path}
                        onClick={() => navigate(path)}
                        className={cn(
                            "flex flex-col items-center justify-center w-full py-2 px-1 rounded-lg transition-all duration-200",
                            currentPath === path
                                ? "text-primary bg-primary/10"
                                : "text-muted-foreground hover:text-foreground hover:bg-muted/20"
                        )}
                    >
                        <Icon className={cn("w-5 h-5 mb-1", currentPath === path && "fill-current")} />
                        <span className="text-[10px] font-medium truncate max-w-full">
                            {t(labelKey)}
                        </span>
                    </button>
                ))}
            </div>
        </nav>
    );
}
