import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

const SEGMENTS = 48;
const FLOOR_DB = -60;
/** Segments from here up warn that the voice is close to clipping. */
const HOT_DB = -9;
const SCALE = [-60, -48, -36, -24, -12, 0];

const toDb = (peak: number) => (peak > 0 ? 20 * Math.log10(peak) : -Infinity);

/**
 * The phone's voice as a deck's segment meter, with a peak hold that falls
 * slowly so short syllables stay visible.
 */
export function LevelMeter({ level, active, className }: { level: number; active: boolean; className?: string }) {
  const [shown, setShown] = useState(0);
  const levelRef = useRef(level);
  levelRef.current = active ? level : 0;

  useEffect(() => {
    let hold = 0;
    let frame = requestAnimationFrame(function tick() {
      const pct = Math.max(0, Math.min(1, (toDb(levelRef.current) - FLOOR_DB) / -FLOOR_DB));
      const next = Math.max(pct, hold * 0.975);
      hold = next < 0.002 ? 0 : next;
      setShown((prev) => (Math.abs(prev - hold) > 0.004 || (hold === 0 && prev !== 0) ? hold : prev));
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  const lit = Math.round(shown * SEGMENTS);
  const db = toDb(active ? level : 0);

  return (
    <div className={cn("flex flex-col gap-2.5", className)}>
      <div className="flex items-baseline justify-between">
        <span className="font-display text-sm font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Voice level
        </span>
        <span data-slot="figure" className="font-mono text-sm font-bold">
          {Number.isFinite(db) && db > FLOOR_DB ? `${db.toFixed(0)} dBFS` : "silent"}
        </span>
      </div>
      <div
        role="meter"
        aria-label="Voice level"
        aria-valuemin={0}
        aria-valuemax={1}
        aria-valuenow={Number(shown.toFixed(2))}
        className="grid h-14 gap-1"
        style={{ gridTemplateColumns: `repeat(${SEGMENTS}, minmax(0, 1fr))` }}
      >
        {Array.from({ length: SEGMENTS }, (_, i) => {
          const segmentDb = FLOOR_DB + ((i + 1) / SEGMENTS) * -FLOOR_DB;
          const on = i < lit;
          return (
            <i
              key={i}
              className={cn(
                "rounded-[2px] transition-colors duration-75",
                !on && "bg-secondary",
                on && (segmentDb > HOT_DB ? "bg-ready" : "bg-foreground"),
              )}
            />
          );
        })}
      </div>
      <div data-slot="figure" className="flex justify-between font-mono text-xs text-faint">
        {SCALE.map((v) => (
          <span key={v}>{v === 0 ? "0 dB" : v}</span>
        ))}
      </div>
    </div>
  );
}
