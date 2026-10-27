import { openUrl } from "@tauri-apps/plugin-opener";
import type { OnAir } from "@/lib/on-air";
import { cn } from "@/lib/utils";

const KEY = [
  { ink: "live", className: "bg-live", label: "On air" },
  { ink: "ready", className: "bg-ready", label: "Connected, no sound" },
  { ink: "wait", className: "bg-wait", label: "Waiting for a phone" },
] as const;

/**
 * The cassette's spine: the wordmark written sideways, the key of the three
 * index inks (the current one lit), and this PC's name and address.
 */
export function Spine({ onAir, host, ip, version }: { onAir: OnAir; host: string; ip?: string; version: string }) {
  return (
    <aside data-tauri-drag-region className="flex h-full flex-col items-center gap-6 border-r bg-spine py-6">
      <ul aria-label="Status key" className="flex flex-col gap-1">
        {KEY.map((k) => (
          <li key={k.ink} title={k.label} aria-current={k.ink === onAir.ink}>
            <i
              className={cn(
                "block h-1.5 w-7 rounded-[1px] transition-opacity duration-700",
                k.className,
                k.ink === onAir.ink ? "opacity-100" : "opacity-25",
              )}
            />
            <span className="sr-only">{k.label}</span>
          </li>
        ))}
      </ul>
      <span
        data-tauri-drag-region
        className="rotate-180 font-display text-[1.75rem] font-bold tracking-[0.04em] [writing-mode:vertical-rl]"
      >
        celmic
      </span>
      <div className="mt-auto flex rotate-180 flex-col gap-1 [writing-mode:vertical-rl]">
        <span data-slot="figure" className="font-mono text-xs text-muted-foreground">
          {host}
          {ip ? ` · ${ip}` : ""}
        </span>
        <span className="text-xs text-faint">
          v{version} · open source ·{" "}
          <button
            className="underline-offset-4 hover:text-foreground hover:underline"
            onClick={() => openUrl("https://github.com/blackfoxsoftware/celmic")}
          >
            GitHub
          </button>
        </span>
      </div>
    </aside>
  );
}
