import { cn } from "@/lib/utils";

const DIGITS = "0123456789";

/**
 * Figures set on a cassette deck's mechanical counter: each digit is a wheel
 * that rolls to its new value. Anything that is not a digit is printed flat.
 */
export function TapeCounter({
  value,
  label,
  className,
  cellClassName,
}: {
  value: string;
  /** Accessible reading of the value, e.g. "5 minutes 24 seconds". */
  label?: string;
  className?: string;
  cellClassName?: string;
}) {
  return (
    <span role="img" aria-label={label ?? value} className={cn("inline-flex items-center gap-[0.12em]", className)}>
      {value.split("").map((char, i) =>
        DIGITS.includes(char) ? (
          <span
            key={i}
            aria-hidden
            className={cn(
              "relative inline-block h-[1.3em] w-[1em] overflow-hidden rounded-sm bg-spine font-mono font-bold leading-[1.3em] text-foreground",
              "shadow-[inset_0_-0.45em_0_rgb(255_255_255/0.035)]",
              cellClassName,
            )}
          >
            <span
              className="absolute inset-x-0 top-0 flex flex-col items-center transition-transform duration-500 ease-out-expo"
              style={{ transform: `translateY(${-Number(char) * 1.3}em)` }}
            >
              {DIGITS.split("").map((d) => (
                <span key={d} className="h-[1.3em]">
                  {d}
                </span>
              ))}
            </span>
          </span>
        ) : (
          <span key={i} aria-hidden className="font-mono font-bold">
            {char}
          </span>
        ),
      )}
    </span>
  );
}

/** "mm:ss" up to an hour, "h:mm:ss" after. */
export function formatCounter(totalSecs: number) {
  const h = Math.floor(totalSecs / 3600);
  const m = Math.floor((totalSecs % 3600) / 60);
  const s = totalSecs % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

/** "5 minutes 24 seconds", for screen readers. */
export function spokenDuration(totalSecs: number) {
  const h = Math.floor(totalSecs / 3600);
  const m = Math.floor((totalSecs % 3600) / 60);
  const s = totalSecs % 60;
  const part = (n: number, unit: string) => (n ? `${n} ${unit}${n === 1 ? "" : "s"}` : "");
  return [part(h, "hour"), part(m, "minute"), part(s, "second")].filter(Boolean).join(" ") || "0 seconds";
}
