import { useCallback, useEffect, useMemo, useState } from "react";
import { TriangleAlert, Info as InfoIcon } from "lucide-react";
import { api, type DeviceInfo, type DriverState, type Info, type Status, type UsbMicState } from "@/api";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { IndexStripe } from "@/components/index-stripe";
import { LevelMeter } from "@/components/level-meter";
import { OutputPanel, isCelmicMic, outputSummary } from "@/components/output-panel";
import { PairCode, PairPanel } from "@/components/pair-panel";
import { Spine } from "@/components/spine";
import { TapeCounter, formatCounter, spokenDuration } from "@/components/tape-counter";
import { TrackList } from "@/components/track-list";
import { TuningPanel } from "@/components/tuning-panel";
import { WindowControls } from "@/components/window-controls";
import { useOnAir } from "@/lib/on-air";

type Tab = "live" | "pair" | "output" | "tuning";

export default function App() {
  const [info, setInfo] = useState<Info | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [driver, setDriver] = useState<DriverState | null>(null);
  const [usbMic, setUsbMic] = useState<UsbMicState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("live");

  const refreshDevices = useCallback(() => {
    api.listOutputDevices().then(setDevices).catch((e) => setError(String(e)));
    api.driverState().then(setDriver).catch(() => setDriver(null));
    api.usbMicState().then(setUsbMic).catch(() => setUsbMic(null));
  }, []);

  useEffect(() => {
    api.getInfo().then(setInfo).catch((e) => setError(String(e)));
    refreshDevices();
    const unlisten = api.onStatus(setStatus);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refreshDevices]);

  // celmic attaches its microphone in the background (at startup, or when
  // usbip-win2 reattaches it): pick up the new device when the output follows.
  const activeOutputId = status?.output?.id;
  useEffect(() => {
    if (activeOutputId) refreshDevices();
  }, [activeOutputId, refreshDevices]);

  // Code and addresses come live with every status event (they change after
  // a safety rotation or a network switch).
  const liveCode = status?.code ?? info?.code;
  const liveIps = (status?.ips ?? info?.ips ?? []).join(",");
  const pairInfo = useMemo(
    () => (info ? { ...info, code: liveCode ?? info.code, ips: liveIps ? liveIps.split(",") : [] } : null),
    [info, liveCode, liveIps],
  );

  const run = (promise: Promise<unknown>) => promise.then(() => setError(null)).catch((e) => setError(String(e)));

  const onAir = useOnAir(status);
  const client = status?.client;
  const problems = [info?.listenError, info?.mdnsError, client ? null : status?.outputError, error].filter(
    (m): m is string => !!m,
  );

  const stripe = client
    ? {
        title: client.name,
        detail: onAir.ink === "ready" ? onAir.detail : `${outputSummary(status)} · from ${client.address}`,
        counter: (
          <TapeCounter value={formatCounter(client.connectedSecs)} label={spokenDuration(client.connectedSecs)} />
        ),
        counterCaption: "Time on air",
      }
    : {
        title: "Waiting for a phone",
        detail: `Open celmic on the phone and pick ${pairInfo?.name ?? "this PC"}, or scan the code.`,
        counter: (
          <TapeCounter value={pairInfo?.code ?? "000000"} label={`Pairing code ${pairInfo?.code ?? ""}`} />
        ),
        counterCaption: "Pairing code",
      };

  const total = (status?.packets ?? 0) + (status?.lost ?? 0);
  const lossPct = total > 0 ? ((status!.lost / total) * 100).toFixed(1) : "0.0";

  return (
    <div className="grid h-full grid-cols-[4.75rem_minmax(0,1fr)]">
      <Spine onAir={onAir} host={pairInfo?.name ?? "…"} ip={pairInfo?.ips[0]} version={info?.version ?? ""} />

      <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)} className="flex min-h-0 flex-col gap-0">
        {/* The title bar: celmic draws it instead of Windows (decorations: false). */}
        <header className="flex h-14 shrink-0 items-center gap-2 border-b pl-6">
          <TabsList className="h-auto gap-1 bg-transparent p-0">
            {(["live", "pair", "output", "tuning"] as const).map((t) => (
              <TabsTrigger
                key={t}
                value={t}
                className="h-9 rounded-md px-4 text-[15px] font-semibold capitalize text-muted-foreground focus-visible:ring-0 data-[state=active]:bg-secondary data-[state=active]:text-foreground data-[state=active]:shadow-none"
              >
                {t}
              </TabsTrigger>
            ))}
          </TabsList>
          <span data-tauri-drag-region className="h-full flex-1" />
          {client && (
            <Button variant="outline" className="mr-4" onClick={() => run(api.disconnectClient())}>
              Disconnect
            </Button>
          )}
          <WindowControls />
        </header>

        <IndexStripe onAir={onAir} compact={tab !== "live"} {...stripe} />

        {problems.map((message) => (
          <p key={message} role="alert" className="flex items-start gap-3 border-b bg-ready/12 px-8 py-3">
            <TriangleAlert className="mt-0.5 size-4 shrink-0 text-ready" />
            {message}
          </p>
        ))}
        {notice && (
          <p role="status" className="flex items-start gap-3 border-b px-8 py-3">
            <InfoIcon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
            {notice}
          </p>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto">
          <TabsContent value="live" className="grid min-h-full grid-cols-[minmax(0,1fr)_22rem]">
            <div className="flex flex-col gap-8 px-8 py-8">
              {client ? (
                <>
                  <LevelMeter level={status?.level ?? 0} active />
                  <TrackList
                    side="A"
                    tracks={[
                      { key: "from", label: "From", value: `${client.address} · ${client.sampleRate / 1000} kHz` },
                      { key: "buffer", label: "Buffer", value: `${status!.bufferMs.toFixed(0)} / ${status!.targetLatencyMs} ms` },
                      { key: "loss", label: "Packet loss", value: `${lossPct} %` },
                      { key: "underruns", label: "Underruns", value: String(status!.underruns) },
                    ]}
                  />
                </>
              ) : (
                <PairCode info={pairInfo} size="lg" showCode={false} className="my-auto" />
              )}
            </div>
            <aside className="flex flex-col gap-8 border-l bg-spine/60 px-7 py-8">
              <section className="flex flex-col gap-2">
                <h2 className="font-display text-lg font-semibold uppercase tracking-[0.08em]">Output</h2>
                <p className="text-lg font-semibold">
                  {status?.output ? (isCelmicMic(status.output.id) ? "Celmic Microphone" : status.output.name) : "None"}
                </p>
                <p className="text-sm text-muted-foreground">
                  {status?.virtualMic
                    ? status.virtualMic.activeStreams > 0
                      ? `In use by ${status.virtualMic.activeStreams} app${status.virtualMic.activeStreams > 1 ? "s" : ""}.`
                      : "No app is recording from it right now."
                    : "Apps can record it as a microphone."}
                </p>
                <button
                  className="self-start text-sm text-muted-foreground underline underline-offset-4 hover:text-foreground"
                  onClick={() => setTab("output")}
                >
                  Change output
                </button>
              </section>
              {client && (
                <section className="flex flex-col gap-3 border-t pt-6">
                  <h2 className="font-display text-lg font-semibold uppercase tracking-[0.08em]">Pair another phone</h2>
                  <PairCode info={pairInfo} size="sm" />
                </section>
              )}
            </aside>
          </TabsContent>

          <TabsContent value="pair" className="px-8 py-8">
            <PairPanel
              info={pairInfo}
              rotatedForSafety={!!status?.codeRotatedForSafety}
              onRegenerate={() => api.regenerateCode().catch((e) => setError(String(e)))}
            />
          </TabsContent>

          <TabsContent value="output" className="px-8 py-8">
            <OutputPanel
              status={status}
              devices={devices}
              driver={driver}
              usbMic={usbMic}
              onRefresh={refreshDevices}
              onSelect={(id) => run(api.setOutputDevice(id))}
              onCreateVirtual={() => run(api.createVirtualMic().then(refreshDevices))}
              onSetupUsbMic={() =>
                api
                  .setupUsbMic()
                  .then((reboot) => {
                    setError(null);
                    setNotice(reboot ? "Restart Windows if Celmic Microphone does not show up in your apps." : null);
                  })
                  .catch((e) => setError(String(e)))
                  .finally(refreshDevices)
              }
              onSetDriverInstalled={(install) =>
                api
                  .setDriverInstalled(install)
                  .then((reboot) => {
                    setError(null);
                    setNotice(reboot ? "Windows needs a restart to finish setting up the driver." : null);
                  })
                  .catch((e) => setError(String(e)))
                  .finally(refreshDevices)
              }
            />
          </TabsContent>

          <TabsContent value="tuning" className="px-8 py-8">
            <TuningPanel status={status} />
          </TabsContent>
        </div>
      </Tabs>
    </div>
  );
}
