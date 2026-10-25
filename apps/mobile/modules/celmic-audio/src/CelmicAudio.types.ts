export type StreamStateName = "idle" | "connecting" | "streaming" | "reconnecting" | "error";

export type StreamErrorReason =
  | "bad_code"
  | "timeout"
  | "unsupported_version"
  | "mic"
  | "network"
  /** The user disconnected this phone from the desktop app. */
  | "kicked"
  /** Another phone paired with the same desktop. */
  | "replaced";

export type StreamState = {
  state: StreamStateName;
  desktopName?: string | null;
  reason?: StreamErrorReason;
  error?: string;
};

export type StreamStats = {
  /** Peak level since the previous event, 0..1. */
  level: number;
  /** Round-trip time to the desktop in ms, -1 until the first PONG. */
  rttMs: number;
  packetsSent: number;
  sampleRate: number;
};

export type StartOptions = {
  /** Candidate IPv4 addresses; the native side races them. */
  hosts: string[];
  port: number;
  code: string;
  deviceName: string;
  /** Echo cancellation + noise suppression (voice) instead of the raw mic. */
  voiceProcessing: boolean;
};

export type DiscoveredDesktop = {
  name: string;
  hosts: string[];
  port: number;
};

export type PermissionResponse = {
  granted: boolean;
  status: "granted" | "denied" | "undetermined";
  canAskAgain?: boolean;
};

export type CelmicAudioEvents = {
  onStateChange(event: StreamState): void;
  onStats(event: StreamStats): void;
};
