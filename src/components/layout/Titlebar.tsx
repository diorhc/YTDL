import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Minus, Square, X, Copy } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface TitleBarProps {
  platform?: string;
}

export function TitleBar({ platform }: TitleBarProps) {
  const [isMaximized, setIsMaximized] = useState(false);
  const isMac = platform === "macos";

  useEffect(() => {
    const appWindow = getCurrentWindow();
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
      unlisten.then((fn) => fn());
    };
  }, []);

  if (isMac) {
    return <div className="titlebar-drag h-10" />;
  }

  const appWindow = getCurrentWindow();

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
