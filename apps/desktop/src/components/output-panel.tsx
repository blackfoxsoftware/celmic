import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Mic, RefreshCw } from "lucide-react";
import { USB_MIC_ID, VIRTUAL_MIC_ID, type DeviceInfo, type DriverState, type Status, type UsbMicState } from "@/api";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { TrackList } from "@/components/track-list";

const PLATFORM = navigator.userAgent.includes("Windows")
  ? "windows"
  : navigator.userAgent.includes("Mac")
    ? "macos"
    : "linux";

const VIRTUAL_CABLE_HELP = {
  windows: { name: "VB-CABLE", url: "https://vb-audio.com/Cable/" },
  macos: { name: "BlackHole", url: "https://existential.audio/blackhole/" },
  linux: null,
} as const;

const AUTOMATIC = "auto";

export function isCelmicMic(id: string | undefined) {
  return id === VIRTUAL_MIC_ID || id === USB_MIC_ID;
}

/** One line on which microphone the apps should pick. */
export function outputSummary(status: Status | null) {
  const active = status?.output;
  if (!active) return "No output open";
  if (isCelmicMic(active.id)) return "into Celmic Microphone";
  return `into ${active.name}`;
}

export function OutputPanel({
  status,
  devices,
  driver,
  usbMic,
  onRefresh,
  onSelect,
  onCreateVirtual,
  onSetupUsbMic,
  onSetDriverInstalled,
}: {
  status: Status | null;
  devices: DeviceInfo[];
  driver: DriverState | null;
  usbMic: UsbMicState | null;
  onRefresh: () => void;
  onSelect: (id: string | null) => void;
  onCreateVirtual: () => void;
  onSetupUsbMic: () => Promise<unknown>;
  onSetDriverInstalled: (install: boolean) => Promise<unknown>;
}) {
  const [busy, setBusy] = useState(false);
  const active = status?.output;
  const help = VIRTUAL_CABLE_HELP[PLATFORM];
  const hasCable = devices.some((d) => d.isVirtual && !isCelmicMic(d.id));
  const usingCelmicMic = isCelmicMic(active?.id);
  const offerUsbMic = usbMic?.supported && !usbMic.attached && !usingCelmicMic;
  // The kernel driver is test-signed for now: offered only by builds that ship it.
  const offerDriver = driver?.supported && !driver.installed && driver.packageAvailable && !offerUsbMic;

  const whileBusy = (work: Promise<unknown>) => {
    setBusy(true);
    work.finally(() => setBusy(false));
  };

  return (
    <div className="flex max-w-3xl flex-col gap-8">
      <div className="flex flex-col gap-3">
        <label htmlFor="output-device" className="font-display text-lg font-semibold uppercase tracking-[0.08em]">
          Where the phone plays
        </label>
        <div className="flex gap-3">
          <Select value={active?.id ?? AUTOMATIC} onValueChange={(v) => onSelect(v === AUTOMATIC ? null : v)}>
            <SelectTrigger id="output-device" className="h-11 flex-1 text-base">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={AUTOMATIC}>Automatic</SelectItem>
              {devices.map((d) => (
                <SelectItem key={d.id} value={d.id}>
                  {d.name}
                  {d.isVirtual && !isCelmicMic(d.id) ? "  · virtual cable" : ""}
                  {d.isDefault ? "  · default" : ""}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button variant="outline" size="lg" className="h-11" onClick={onRefresh}>
            <RefreshCw />
            Refresh
          </Button>
        </div>
      </div>

      {usingCelmicMic && (
        <p className="flex items-start gap-3 text-lg">
          <Mic className="mt-1 size-5 shrink-0 text-live" />
          <span>
            Apps hear your phone as <strong>Celmic Microphone</strong>. Pick it as the input in Discord, Zoom, OBS or
            the Windows sound settings.
          </span>
        </p>
      )}

      {offerUsbMic && (
        <div className="flex flex-col items-start gap-4 border-y py-6">
          {usbMic.installed ? (
            <p className="text-lg">Connecting Celmic Microphone to Windows…</p>
          ) : (
            <p className="max-w-[60ch] text-lg">
              Set up <strong>Celmic Microphone</strong> so every app sees your phone as a real microphone, with no
              virtual cable. celmic installs the free, open-source{" "}
              <button
                className="underline decoration-faint underline-offset-4 hover:decoration-foreground"
                onClick={() => openUrl("https://github.com/vadimgrn/usbip-win2")}
              >
                usbip-win2
              </button>{" "}
              (signed by Microsoft); Windows asks for permission once.
            </p>
          )}
          {usbMic.error ? (
            <p className="text-ready">{usbMic.error}</p>
          ) : (
            <Button size="lg" disabled={busy} onClick={() => whileBusy(onSetupUsbMic())}>
              {busy ? "Setting up…" : usbMic.installed ? "Connect now" : "Set up Celmic Microphone"}
            </Button>
          )}
          {help && !hasCable && (
            <button className="text-sm text-muted-foreground underline-offset-4 hover:underline" onClick={() => openUrl(help.url)}>
              or use {help.name} instead
            </button>
          )}
        </div>
      )}

      {offerDriver && (
        <div className="flex flex-col items-start gap-4 border-y py-6">
          <p className="max-w-[60ch]">
            Install the test-signed <strong>Celmic Microphone</strong> kernel driver. It only loads in Windows test
            mode; Windows asks for administrator permission.
          </p>
          <Button variant="outline" disabled={busy} onClick={() => whileBusy(onSetDriverInstalled(true))}>
            {busy ? "Waiting for Windows…" : "Install kernel driver"}
          </Button>
        </div>
      )}

      {active && !active.isVirtual && !offerDriver && !offerUsbMic && (
        <div className="flex flex-col items-start gap-4 border-y py-6">
          <p className="max-w-[60ch]">
            Audio is playing on <strong>{active.name}</strong>. To use it as a microphone in other apps, route it
            into a virtual cable.
          </p>
          {help ? (
            !hasCable && (
              <Button onClick={() => openUrl(help.url)}>Get {help.name}</Button>
            )
          ) : (
            <Button onClick={onCreateVirtual}>Create virtual mic</Button>
          )}
        </div>
      )}

      {active && (
        <TrackList
          side="B"
          tracks={[
            { key: "device", label: "Device", value: usingCelmicMic ? "Celmic Microphone" : active.name },
            { key: "format", label: "Format", value: `${active.sampleRate / 1000} kHz · ${active.channels} ch` },
            ...(status?.virtualMic
              ? [
                  {
                    key: "apps",
                    label: "Apps recording",
                    value: String(status.virtualMic.activeStreams),
                    tone: status.virtualMic.activeStreams === 0 ? ("ready" as const) : undefined,
                  },
                ]
              : []),
          ]}
        />
      )}

      {driver?.installed && (
        <button
          className="self-start text-sm text-muted-foreground underline-offset-4 hover:underline disabled:opacity-50"
          disabled={busy}
          onClick={() => whileBusy(onSetDriverInstalled(false))}
        >
          Remove the Celmic Microphone kernel driver
        </button>
      )}
    </div>
  );
}
