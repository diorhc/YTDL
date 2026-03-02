import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Minus, Square, X, Copy } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform as getPlatform } from "@tauri-apps/plugin-os";

interface TitleBarProps {
  platform?: string;
}

type WindowControls = {
  isMaximized: () => Promise<boolean>;
  onResized: (handler: () => void | Promise<void>) => Promise<() => void>;
  minimize: () => Promise<void>;
  toggleMaximize: () => Promise<void>;
  close: () => Promise<void>;
};

function tryGetCurrentWindow(): WindowControls | null {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

export function TitleBar({ platform }: TitleBarProps) {
  const [isMaximized, setIsMaximized] = useState(false);
  const detectedPlatform =
    platform ||
    (() => {
      try {
        return getPlatform();
      } catch {
        return "web";
      }
    })();

  const isMac = detectedPlatform === "macos";
  const isMobile = detectedPlatform === "android" || detectedPlatform === "ios";

  useEffect(() => {
    if (isMobile || isMac) {
      return;
    }

    const appWindow = tryGetCurrentWindow();
    if (!appWindow) {
      return;
    }

    const checkMaximized = async () => {
      const maximized = await appWindow.isMaximized();
      setIsMaximized(maximized);
    };
    checkMaximized();

    const unlisten = appWindow.onResized(async () => {
      const maximized = await appWindow.isMaximized();
      setIsMaximized(maximized);
    });

    return () => {
      unlisten.then((fn: () => void) => fn());
    };
  }, [isMac, isMobile]);

  if (isMobile) {
    return null;
  }

  if (isMac) {
    return <div className="titlebar-drag h-10" />;
  }

  const fallbackWindow: WindowControls = {
    isMaximized: async () => false,
    onResized: async () => () => {},
    minimize: async () => {},
    toggleMaximize: async () => {},
    close: async () => {},
  };

  const appWindow = tryGetCurrentWindow() ?? fallbackWindow;

  return (
    <div className="titlebar-drag flex justify-end pt-3 pr-4">
      <div className="titlebar-no-drag flex items-center gap-1">
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 rounded-lg hover:bg-muted"
          onClick={() => appWindow.minimize()}
        >
          <Minus className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 rounded-lg hover:bg-muted"
          onClick={() => appWindow.toggleMaximize()}
        >
          {isMaximized ? (
            <Copy className="h-3.5 w-3.5 rotate-180" />
          ) : (
            <Square className="h-3.5 w-3.5" />
          )}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 rounded-lg hover:bg-red-500 hover:text-white"
          onClick={() => appWindow.close()}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
