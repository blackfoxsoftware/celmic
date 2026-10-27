import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export type Track = { key: string; label: string; value: ReactNode; tone?: "ready" };

/**
 * Facts set like a J-card's track listing: side letter and number, a label,
 * and the figure, packed edge to edge between hairlines.
 */
export function TrackList({ side, tracks, className }: { side: "A" | "B"; tracks: Track[]; className?: string }) {
  return (
    <dl className={cn("border-t", className)}>
      {tracks.map((track, i) => (
        <div key={track.key} className="grid grid-cols-[3rem_minmax(0,1fr)_auto] items-baseline gap-3 border-b py-3">
          <dt className="contents">
            <span data-slot="figure" className="font-mono text-[13px] text-faint">
              {side}
              {i + 1}
            </span>
            <span className="text-[15px] font-medium text-muted-foreground">{track.label}</span>
          </dt>
          <dd
            data-slot="figure"
            className={cn("truncate text-right font-mono text-base font-bold", track.tone === "ready" && "text-ready")}
          >
            {track.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}
