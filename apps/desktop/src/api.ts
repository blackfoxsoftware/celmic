import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Info = {
  name: string;
  port: number;
  code: string;
  ips: string[];
  version: string;
  listenError: string | null;
  mdnsError: string | null;
};

export type ClientInfo = {
  name: string;
  address: string;
  sampleRate: number;
  connectedSecs: number;
};

export type ActiveOutput = {
  id: string;
  name: string;
  sampleRate: number;
  channels: number;
  isVirtual: boolean;
};

export type DeviceInfo = {
  id: string;
  name: string;
  isDefault: boolean;
  isVirtual: boolean;
};

export type VirtualMicInfo = {
  /** Apps currently recording from "Celmic Microphone". */
  activeStreams: number;
  queuedMs: number;
  underruns: number;
};

export type DriverState = {
  supported: boolean;
  installed: boolean;
  packageAvailable: boolean;
};

export const VIRTUAL_MIC_ID = "celmic:virtual-mic";

export type Status = {
  /** Current pairing code; changes after a manual or safety rotation. */
  code: string;
  /** Wrong codes were entered too often and the code was rotated. */
  codeRotatedForSafety: boolean;
  /** Current LAN addresses (refreshed when the network changes). */
  ips: string[];
  client: ClientInfo | null;
  level: number;
  packets: number;
  lost: number;
  underruns: number;
  bufferMs: number;
  targetLatencyMs: number;
  gain: number;
  output: ActiveOutput | null;
  outputError: string | null;
  virtualMic: VirtualMicInfo | null;
};

export const api = {
  getInfo: () => invoke<Info>("get_info"),
  regenerateCode: () => invoke<string>("regenerate_code"),
  listOutputDevices: () => invoke<DeviceInfo[]>("list_output_devices"),
  setOutputDevice: (id: string | null) => invoke<ActiveOutput>("set_output_device", { id }),
  setGain: (gain: number) => invoke<void>("set_gain", { gain }),
  setLatency: (ms: number) => invoke<void>("set_latency", { ms }),
  disconnectClient: () => invoke<void>("disconnect_client"),
  createVirtualMic: () => invoke<ActiveOutput>("create_virtual_mic"),
  driverState: () => invoke<DriverState>("driver_state"),
  /** Shows a UAC prompt. Resolves to whether Windows wants a reboot. */
  setDriverInstalled: (install: boolean) => invoke<boolean>("set_driver_installed", { install }),
  onStatus: (cb: (status: Status) => void): Promise<UnlistenFn> =>
    listen<Status>("status", (event) => cb(event.payload)),
};
