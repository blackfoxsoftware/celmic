import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { cn } from "@/lib/utils";

const appWindow = getCurrentWindow();

/** Minimize, maximize/restore and close, drawn by celmic instead of Windows. */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const sync = () => appWindow.isMaximized().then(setMaximized);
    sync();
    const unlisten = appWindow.onResized(sync);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const button = "grid h-full w-12 place-items-center text-muted-foreground transition-colors duration-150";

  return (
    <div className="flex h-full shrink-0 items-stretch">
      <button aria-label="Minimize" className={cn(button, "hover:bg-secondary hover:text-foreground")} onClick={() => appWindow.minimize()}>
        <Minus className="size-4" strokeWidth={1.75} />
      </button>
      <button
        aria-label={maximized ? "Restore" : "Maximize"}
        className={cn(button, "hover:bg-secondary hover:text-foreground")}
        onClick={() => appWindow.toggleMaximize()}
      >
        {maximized ? <Copy className="size-3.5 -scale-x-100" strokeWidth={1.75} /> : <Square className="size-3.5" strokeWidth={1.75} />}
      </button>
      <button
        aria-label="Close"
        className={cn(button, "hover:bg-live hover:text-live-foreground")}
        onClick={() => appWindow.close()}
      >
        <X className="size-4" strokeWidth={1.75} />
      </button>
    </div>
  );
}
