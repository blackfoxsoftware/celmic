import { NativeModule, requireNativeModule } from "expo";

import type {
  CelmicAudioEvents,
  DiscoveredDesktop,
  PermissionResponse,
  StartOptions,
  StreamState,
} from "./CelmicAudio.types";

declare class CelmicAudioModule extends NativeModule<CelmicAudioEvents> {
  requestPermissionsAsync(): Promise<PermissionResponse>;
  requestNotificationPermissionsAsync(): Promise<PermissionResponse>;
  start(options: StartOptions): void;
  stop(): void;
  setMuted(muted: boolean): void;
  getState(): StreamState;
  discoverAsync(timeoutMs: number): Promise<DiscoveredDesktop[]>;
}

export default requireNativeModule<CelmicAudioModule>("CelmicAudio");
