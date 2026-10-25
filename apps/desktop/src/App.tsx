import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { QRCodeSVG } from "qrcode.react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { buildConnectUrl } from "@celmic/protocol";
import { api, VIRTUAL_MIC_ID, type DeviceInfo, type DriverState, type Info, type Status } from "./api";

const PLATFORM = navigator.userAgent.includes("Windows")
  ? "windows"
  : navigator.userAgent.includes("Mac")
    ? "macos"
    : "linux";

const VIRTUAL_CABLE_HELP = {
  windows: { label: "Get VB-CABLE", url: "https://vb-audio.com/Cable/" },
  macos: { label: "Get BlackHole", url: "https://existential.audio/blackhole/" },
  linux: null,
} as const;

export default function App() {
  const [info, setInfo] = useState<Info | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [driver, setDriver] = useState<DriverState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refreshDevices = useCallback(() => {
    api.listOutputDevices().then(setDevices).catch((e) => setError(String(e)));
    api.driverState().then(setDriver).catch(() => setDriver(null));
  }, []);

  useEffect(() => {
    api.getInfo().then(setInfo).catch((e) => setError(String(e)));
    refreshDevices();
    const unlisten = api.onStatus(setStatus);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refreshDevices]);

  // Code and addresses come live with every status event (they can change
  // after a safety rotation or a network switch); memoized so the QR only
  // re-renders when they actually change.
  const liveCode = status?.code ?? info?.code;
  const liveIps = (status?.ips ?? info?.ips ?? []).join(",");
  const pairInfo = useMemo(
    () => (info ? { ...info, code: liveCode ?? info.code, ips: liveIps ? liveIps.split(",") : [] } : null),
    [info, liveCode, liveIps],
  );

  const onRegenerate = useCallback(() => {
    api.regenerateCode().catch((e) => setError(String(e)));
  }, []);

  const run = (promise: Promise<unknown>) =>
    promise.then(() => setError(null)).catch((e) => setError(String(e)));

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <img src="/celmic.svg" alt="" width={28} height={28} />
          <span>celmic</span>
        </div>
        <ConnectionPill status={status} />
      </header>

      {[info?.listenError, info?.mdnsError, status?.outputError, error]
        .filter(Boolean)
        .map((message) => (
          <div className="banner" role="alert" key={message}>
            {message}
          </div>
        ))}
      {status?.codeRotatedForSafety && (
        <div className="banner" role="alert">
          Someone entered too many wrong pairing codes, so Celmic changed the code. Pair your phone again with the new
          one.
        </div>
      )}
      {notice && (
        <div className="banner banner-info" role="status">
          {notice}
        </div>
      )}

      <main className="grid">
        <PairCard info={pairInfo} onRegenerate={onRegenerate} />

        <div className="stack">
          <LiveCard status={status} onDisconnect={() => run(api.disconnectClient())} />
          <OutputCard
            status={status}
            devices={devices}
            onRefresh={refreshDevices}
            onSelect={(id) => run(api.setOutputDevice(id))}
            onCreateVirtual={() => run(api.createVirtualMic().then(refreshDevices))}
            driver={driver}
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
          <TuningCard status={status} />
        </div>
      </main>

      <footer className="footer">
        Celmic {info?.version ?? ""} · open source ·{" "}
        <button className="link" onClick={() => openUrl("https://github.com/blackfoxsoftware/celmic")}>
          GitHub
        </button>
      </footer>
    </div>
  );
}

function ConnectionPill({ status }: { status: Status | null }) {
  const client = status?.client;
  return (
    <div className={`pill ${client ? "pill-live" : ""}`}>
      <span className="dot" />
      {client ? `Streaming from ${client.name}` : "Waiting for a phone"}
    </div>
  );
}

const PairCard = memo(function PairCard({ info, onRegenerate }: { info: Info | null; onRegenerate: () => void }) {
  const url = useMemo(
    () =>
      info && info.ips.length > 0
        ? buildConnectUrl({ hosts: info.ips.slice(0, 4), port: info.port, code: info.code, name: info.name })
        : null,
    [info],
  );

  return (
    <section className="card pair">
      <h2>Pair a phone</h2>
      <p className="muted">
        Scan with the Celmic app (or your camera), or pick <strong>{info?.name ?? "this PC"}</strong> in the
        app and type the code.
      </p>

      <div className="qr">
        {url ? (
          <QRCodeSVG value={url} size={208} marginSize={2} level="M" />
        ) : (
          <div className="qr-empty">{info ? "No network connection" : "…"}</div>
        )}
      </div>

      <div className="code" aria-label="Pairing code">
        {(info?.code ?? "······").split("").map((digit, i) => (
          <span key={i}>{digit}</span>
        ))}
      </div>
      <button className="ghost" onClick={onRegenerate}>
        New code
      </button>

      <dl className="addresses">
        <dt>Address</dt>
        <dd>{info?.ips.length ? info.ips.map((ip) => <code key={ip}>{ip}</code>) : "—"}</dd>
        <dt>UDP port</dt>
        <dd>
          <code>{info?.port ?? "—"}</code>
        </dd>
      </dl>
    </section>
  );
});

function LiveCard({ status, onDisconnect }: { status: Status | null; onDisconnect: () => void }) {
  const client = status?.client;
  const total = (status?.packets ?? 0) + (status?.lost ?? 0);
  const lossPct = total > 0 ? ((status!.lost / total) * 100).toFixed(1) : "0.0";

  return (
    <section className="card">
      <div className="card-head">
        <h2>Live</h2>
        {client && (
          <button className="ghost small" onClick={onDisconnect}>
            Disconnect
          </button>
        )}
      </div>

      <LevelMeter level={client ? (status?.level ?? 0) : 0} />

      {client ? (
        <dl className="stats">
          <Stat label="Device" value={client.name} />
          <Stat label="From" value={client.address} />
          <Stat label="Buffer" value={`${status!.bufferMs.toFixed(0)} / ${status!.targetLatencyMs} ms`} />
          <Stat label="Loss" value={`${lossPct}%`} />
          <Stat label="Underruns" value={String(status!.underruns)} />
          <Stat label="Uptime" value={formatDuration(client.connectedSecs)} />
        </dl>
      ) : (
        <p className="muted">No phone connected. Audio will start as soon as one pairs.</p>
      )}

      {status?.virtualMic && (
        <p className="muted small">
          {status.virtualMic.activeStreams > 0
            ? `Celmic Microphone is in use by ${status.virtualMic.activeStreams} app${status.virtualMic.activeStreams > 1 ? "s" : ""}.`
            : "No app is recording from Celmic Microphone right now."}
        </p>
      )}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <dt>{label}</dt>
      <dd title={value}>{value}</dd>
    </div>
  );
}

function LevelMeter({ level }: { level: number }) {
  // Peak-hold with a slow fall so short syllables stay visible. The fall runs
  // per animation frame, so the bar drops even when the level stops changing.
  const [shown, setShown] = useState(0);
  const levelRef = useRef(level);
  levelRef.current = level;
  useEffect(() => {
    let hold = 0;
    let frame = requestAnimationFrame(function tick() {
      const db = levelRef.current > 0 ? 20 * Math.log10(levelRef.current) : -Infinity;
      const pct = Math.max(0, Math.min(1, (db + 60) / 60));
      const next = Math.max(pct, hold * 0.98);
      if (Math.abs(next - hold) > 0.002 || (next === 0 && hold !== 0)) setShown(next);
      hold = next < 0.002 ? 0 : next;
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <div className="meter" role="meter" aria-valuemin={0} aria-valuemax={1} aria-valuenow={shown}>
      <div className="meter-fill" style={{ clipPath: `inset(0 ${(1 - shown) * 100}% 0 0)` }} />
      <div className="meter-ticks" />
    </div>
  );
}

function OutputCard({
  status,
  devices,
  driver,
  onRefresh,
  onSelect,
  onCreateVirtual,
  onSetDriverInstalled,
}: {
  status: Status | null;
  devices: DeviceInfo[];
  driver: DriverState | null;
  onRefresh: () => void;
  onSelect: (id: string | null) => void;
  onCreateVirtual: () => void;
  onSetDriverInstalled: (install: boolean) => Promise<unknown>;
}) {
  const [busy, setBusy] = useState(false);
  const active = status?.output;
  const help = VIRTUAL_CABLE_HELP[PLATFORM];
  const hasVirtual = devices.some((d) => d.isVirtual);
  const usingDriver = active?.id === VIRTUAL_MIC_ID;
  const offerDriver = driver?.supported && !driver.installed;

  const setDriver = (install: boolean) => {
    setBusy(true);
    onSetDriverInstalled(install).finally(() => setBusy(false));
  };

  return (
    <section className="card">
      <div className="card-head">
        <h2>Output</h2>
        <button className="ghost small" onClick={onRefresh}>
          Refresh
        </button>
      </div>

      <select value={active?.id ?? ""} onChange={(e) => onSelect(e.target.value || null)}>
        <option value="">Automatic</option>
        {devices.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name}
            {d.isVirtual && d.id !== VIRTUAL_MIC_ID ? "  · virtual cable" : ""}
            {d.isDefault ? "  · default" : ""}
          </option>
        ))}
      </select>

      {active && !usingDriver && (
        <p className="muted small">
          {active.sampleRate / 1000} kHz · {active.channels} ch
        </p>
      )}

      {usingDriver && (
        <div className="hint hint-ok">
          <p>
            Other apps see your phone as <strong>Celmic Microphone</strong>. Pick it as the input in Discord, Zoom,
            OBS or Windows sound settings.
          </p>
        </div>
      )}

      {offerDriver && (
        <div className="hint">
          <p>
            Install the <strong>Celmic Microphone</strong> driver so apps see your phone as a real microphone — no
            virtual cable needed. Windows will ask for administrator permission.
          </p>
          {driver.packageAvailable ? (
            <button className="primary small" disabled={busy} onClick={() => setDriver(true)}>
              {busy ? "Waiting for Windows…" : "Install Celmic Microphone"}
            </button>
          ) : (
            <p className="small">
              Driver package not found in this build. Run <code>bun run driver:build</code> or use a release build.
            </p>
          )}
          {help && !hasVirtual && (
            <button className="link small" onClick={() => openUrl(help.url)}>
              or use {help.label.replace("Get ", "")} instead
            </button>
          )}
        </div>
      )}

      {active && !active.isVirtual && !offerDriver && (
        <div className="hint">
          <p>
            Audio is playing on <strong>{active.name}</strong>. To use it as a microphone in other apps, route it into
            a virtual cable.
          </p>
          {help ? (
            !hasVirtual && (
              <button className="primary small" onClick={() => openUrl(help.url)}>
                {help.label}
              </button>
            )
          ) : (
            <button className="primary small" onClick={onCreateVirtual}>
              Create virtual mic
            </button>
          )}
        </div>
      )}

      {driver?.installed && (
        <button className="link small muted" disabled={busy} onClick={() => setDriver(false)}>
          Remove Celmic Microphone driver
        </button>
      )}
    </section>
  );
}

function TuningCard({ status }: { status: Status | null }) {
  const [gain, setGain] = useState(1);
  const [latency, setLatency] = useState(40);
  const synced = useRef(false);

  useEffect(() => {
    if (status && !synced.current) {
      synced.current = true;
      setGain(status.gain);
      setLatency(status.targetLatencyMs);
    }
  }, [status]);

  return (
    <section className="card">
      <h2>Tuning</h2>
      <label className="slider">
        <span>Gain</span>
        <input
          type="range"
          min={0}
          max={4}
          step={0.05}
          value={gain}
          onChange={(e) => {
            const value = Number(e.target.value);
            setGain(value);
            api.setGain(value);
          }}
        />
        <output>{gain === 0 ? "muted" : `${(20 * Math.log10(gain)).toFixed(1)} dB`}</output>
      </label>
      <label className="slider">
        <span>Latency</span>
        <input
          type="range"
          min={20}
          max={200}
          step={10}
          value={latency}
          onChange={(e) => {
            const value = Number(e.target.value);
            setLatency(value);
            api.setLatency(value);
          }}
        />
        <output>{latency} ms</output>
      </label>
      <p className="muted small">Raise latency if you hear crackles on a busy Wi-Fi network.</p>
    </section>
  );
}

function formatDuration(secs: number) {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m > 0 ? `${m}m ${s.toString().padStart(2, "0")}s` : `${s}s`;
}
