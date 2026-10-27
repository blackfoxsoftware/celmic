import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { INK_CLASS, type OnAir } from "@/lib/on-air";

const TAG = { wait: "Pair", live: "Rec", ready: "Check" } as const;

/**
 * The J-card's index stripe, full width: its ink is the state. The tag sits
 * on the spine edge like a cassette's side label, the name is written large,
 * and the counter on the right reads time on air (or the pairing code).
 */
export function IndexStripe({
  onAir,
  title,
  detail,
  counter,
  counterCaption,
  compact,
}: {
  onAir: OnAir;
  title: string;
  detail: ReactNode;
  counter: ReactNode;
  counterCaption: string;
  compact: boolean;
}) {
  return (
    <section
      aria-live="polite"
      className={cn(
        "relative grid shrink-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-7 px-8",
        "transition-[background-color,color,padding] duration-700 ease-out-expo",
        INK_CLASS[onAir.ink],
        compact ? "py-4" : "py-8",
      )}
    >
      <span
        className={cn(
          "flex items-center gap-2.5 self-stretch border-r-[1px] border-current/25 pr-6 font-display font-bold uppercase tracking-[0.2em]",
          compact ? "text-base" : "text-xl",
        )}
      >
        <i
          className={cn(
            "size-3.5 rounded-full bg-current",
            onAir.ink === "live" && "motion-safe:animate-[pulse_2.4s_ease-in-out_infinite]",
          )}
        />
        {TAG[onAir.ink]}
      </span>

      <div className="min-w-0">
        <h1
          className={cn(
            "truncate font-display font-bold leading-[0.92] tracking-[-0.01em] transition-[font-size] duration-700 ease-out-expo",
            compact ? "text-[2.5rem]" : "text-[clamp(3.5rem,7.2vw,6rem)]",
          )}
        >
          {title}
        </h1>
        <p className={cn("mt-2 truncate font-medium opacity-90", compact ? "text-sm" : "text-lg")}>{detail}</p>
      </div>

      <div className="flex flex-col items-end gap-1.5">
        <div className={cn(compact ? "text-2xl" : "text-[2.75rem]", "transition-[font-size] duration-700 ease-out-expo")}>
          {counter}
        </div>
        <span className="font-display text-xs font-semibold uppercase tracking-[0.16em] opacity-90">
          {counterCaption}
        </span>
      </div>
    </section>
  );
}
