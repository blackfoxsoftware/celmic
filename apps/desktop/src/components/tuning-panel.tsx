import { useEffect, useRef, useState } from "react";
import { api, type Status } from "@/api";
import { Slider } from "@/components/ui/slider";

function Control({
  id,
  label,
  value,
  readout,
  hint,
  min,
  max,
  step,
  onChange,
}: {
  id: string;
  label: string;
  value: number;
  readout: string;
  hint: string;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_7rem] items-center gap-x-6 gap-y-2 border-b py-6">
      <label htmlFor={id} className="font-display text-lg font-semibold uppercase tracking-[0.08em]">
        {label}
      </label>
      <output htmlFor={id} data-slot="figure" className="text-right font-mono text-lg font-bold">
        {readout}
      </output>
      <Slider id={id} min={min} max={max} step={step} value={[value]} onValueChange={([v]) => onChange(v)} />
      <span />
      <p className="text-sm text-muted-foreground">{hint}</p>
    </div>
  );
}

export function TuningPanel({ status }: { status: Status | null }) {
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
    <div className="flex max-w-3xl flex-col border-t">
      <Control
        id="gain"
        label="Gain"
        value={gain}
        readout={gain === 0 ? "muted" : `${(20 * Math.log10(gain)).toFixed(1)} dB`}
        hint="Boost a quiet phone, or bring a loud one down before it clips."
        min={0}
        max={4}
        step={0.05}
        onChange={(v) => {
          setGain(v);
          api.setGain(v);
        }}
      />
      <Control
        id="latency"
        label="Latency"
        value={latency}
        readout={`${latency} ms`}
        hint="Raise it if you hear crackles on a busy Wi-Fi network; lower it for snappier calls."
        min={20}
        max={200}
        step={10}
        onChange={(v) => {
          setLatency(v);
          api.setLatency(v);
        }}
      />
    </div>
  );
}
