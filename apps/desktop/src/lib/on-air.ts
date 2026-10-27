import { useEffect, useRef, useState } from "react";
import type { Status } from "@/api";

/**
 * The three index inks, one state each:
 * - wait: no phone yet (cobalt),
 * - live: a phone is streaming sound (vermilion),
 * - ready: a phone is connected but nothing is reaching the apps (amber).
 */
export type OnAir =
  | { ink: "wait" }
  | { ink: "live" }
  | { ink: "ready"; reason: "silent" | "output"; detail: string };

/** Peak below this (about -50 dBFS) counts as silence. */
const SILENCE = 0.003;
/** How long the phone may be quiet before the stripe turns amber. */
const SILENT_AFTER_MS = 5000;

export function useOnAir(status: Status | null): OnAir {
  const lastSound = useRef(Date.now());
  const [now, setNow] = useState(Date.now());
  const client = status?.client;

  useEffect(() => {
    if (!client || (status?.level ?? 0) > SILENCE) lastSound.current = Date.now();
    setNow(Date.now());
  }, [status, client]);

  if (!client) return { ink: "wait" };
  if (status?.outputError) return { ink: "ready", reason: "output", detail: status.outputError };
  const quietMs = now - lastSound.current;
  if (quietMs > SILENT_AFTER_MS) {
    return {
      ink: "ready",
      reason: "silent",
      detail: `No sound for ${Math.round(quietMs / 1000)} s. Is the phone muted?`,
    };
  }
  return { ink: "live" };
}

export const INK_CLASS = {
  wait: "bg-wait text-wait-foreground",
  live: "bg-live text-live-foreground",
  ready: "bg-ready text-ready-foreground",
} as const;
