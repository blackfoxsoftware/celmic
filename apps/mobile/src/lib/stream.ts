import { useSyncExternalStore } from "react";
import { Platform } from "react-native";
import * as Device from "expo-device";
import type { ConnectTarget } from "@celmic/protocol";

import CelmicAudio, { type StreamState, type StreamStats } from "../../modules/celmic-audio";

type Store = {
  stream: StreamState;
  stats: StreamStats | null;
  target: ConnectTarget | null;
  muted: boolean;
  voiceProcessing: boolean;
  /** When this stream first went live (ms since epoch), for the time-on-air counter. */
  liveSince: number | null;
};

let store: Store = {
  stream: CelmicAudio.getState(),
  stats: null,
  target: null,
  muted: false,
  voiceProcessing: true,
  liveSince: null,
};

const listeners = new Set<() => void>();

function update(patch: Partial<Store>) {
  store = { ...store, ...patch };
  listeners.forEach((listener) => listener());
}

CelmicAudio.addListener("onStateChange", (stream) => {
  const ended = stream.state === "idle" || stream.state === "error";
  if (ended) {
    update({ stream, stats: null, muted: false, liveSince: null });
  } else {
    update({ stream, liveSince: store.liveSince ?? (stream.state === "streaming" ? Date.now() : null) });
  }
});
CelmicAudio.addListener("onStats", (stats) => update({ stats }));

export function useStream(): Store {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => store,
  );
}

export function isActive(state: StreamState["state"]) {
  return state === "connecting" || state === "streaming" || state === "reconnecting";
}

export async function connect(target: ConnectTarget) {
  const permission = await CelmicAudio.requestPermissionsAsync();
  if (!permission.granted) {
    update({
      stream: { state: "error", reason: "mic", error: "Celmic needs microphone access to stream." },
    });
    return;
  }
  if (Platform.OS === "android") {
    // Only affects the "streaming" notification; never blocks.
    await CelmicAudio.requestNotificationPermissionsAsync().catch(() => undefined);
  }

  update({ target, muted: false, stats: null });
  CelmicAudio.start({
    hosts: target.hosts,
    port: target.port,
    code: target.code,
    deviceName: Device.deviceName ?? Device.modelName ?? "Phone",
    voiceProcessing: store.voiceProcessing,
  });
}

export function disconnect() {
  CelmicAudio.stop();
}

export function toggleMute() {
  const muted = !store.muted;
  CelmicAudio.setMuted(muted);
  update({ muted });
}

/** Applies on the next connection. */
export function setVoiceProcessing(voiceProcessing: boolean) {
  update({ voiceProcessing });
}

export function dismissError() {
  if (store.stream.state === "error") update({ stream: { state: "idle" } });
}

export const discover = (timeoutMs = 2500) => CelmicAudio.discoverAsync(timeoutMs);
