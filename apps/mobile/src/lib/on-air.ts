import { useCSSVariable } from "uniwind";

import type { StreamState } from "../../modules/celmic-audio";

/**
 * The three index inks shared with the desktop, one state each:
 * wait (cobalt): not streaming yet; live (vermilion): your voice is going out;
 * ready (amber): connected but silent (muted), or something needs attention.
 */
export type Ink = "wait" | "live" | "ready";

export type OnAir = { ink: Ink; tag: string; title: string; detail: string };

export function onAir(stream: StreamState, muted: boolean, desktop: string | undefined): OnAir {
  switch (stream.state) {
    case "streaming":
      return muted
        ? { ink: "ready", tag: "Muted", title: desktop ?? "Your computer", detail: "Nobody hears you. Tap the key to talk." }
        : { ink: "live", tag: "Rec", title: desktop ?? "Your computer", detail: "Apps on the PC hear this phone." };
    case "connecting":
      return { ink: "wait", tag: "Pair", title: "Connecting…", detail: desktop ? `Reaching ${desktop}` : "Reaching your computer" };
    case "reconnecting":
      return { ink: "ready", tag: "Check", title: desktop ?? "Reconnecting…", detail: "Lost the PC for a moment. Reconnecting…" };
    case "error":
      return { ink: "ready", tag: "Check", title: "Disconnected", detail: stream.error ?? "Something went wrong." };
    default:
      return { ink: "wait", tag: "Pair", title: "Pick your computer", detail: "Scan the code on the PC, or choose it below." };
  }
}

export const INK_CLASS: Record<Ink, { bg: string; text: string; muted: string }> = {
  wait: { bg: "bg-wait", text: "text-wait-foreground", muted: "text-wait-foreground/85" },
  live: { bg: "bg-live", text: "text-live-foreground", muted: "text-live-foreground/85" },
  ready: { bg: "bg-ready", text: "text-ready-foreground", muted: "text-ready-foreground/85" },
};

/** A theme colour as a string, for props that cannot take a className (icons). */
export function useColor(token: string): string {
  return String(useCSSVariable(`--color-${token}`) ?? "#eceae4");
}
