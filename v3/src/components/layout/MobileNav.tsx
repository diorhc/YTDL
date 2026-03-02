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
        <nav className="md:hidden fixed bottom-0 left-0 right-0 z-50 bg-background/90 backdrop-blur-xl border-t border-border/50 pb-[env(safe-area-inset-bottom)]">
            <div className="flex items-center justify-between px-2 pt-2 pb-1 relative overflow-hidden">
                {navItems.map(({ path, icon: Icon, labelKey }) => {
                    const isActive = currentPath === path;
                    return (
                        <button
                            key={path}
                            onClick={() => navigate(path)}
                            className={cn(
                                "relative flex flex-col items-center justify-center flex-1 py-1 sm:py-2 min-h-[48px] rounded-xl transition-all duration-300 z-10",
                                isActive
                                    ? "text-primary-foreground"
                                    : "text-muted-foreground hover:text-foreground"
                            )}
                        >
                            {/* Active background pill */}
                            {isActive && (
                                <div className="absolute inset-0 bg-primary rounded-xl -z-10 transition-all duration-300 shadow-sm" />
                            )}

                            <Icon
                                className={cn(
                                    "w-5 h-5 mb-1 transition-transform duration-300",
                                    isActive && "scale-110"
                                )}
                                strokeWidth={isActive ? 2.5 : 2}
                            />
                            <span className={cn(
                                "text-[9px] sm:text-[10px] font-medium truncate max-w-full transition-opacity duration-300",
                                isActive ? "opacity-100" : "opacity-80"
                            )}>
                                {t(labelKey)}
                            </span>
                        </button>
                    );
                })}
            </div>
        </nav>
    );
}
