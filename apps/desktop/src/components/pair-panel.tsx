import { useMemo } from "react";
import { QRCodeSVG } from "qrcode.react";
import { RefreshCw } from "lucide-react";
import { buildConnectUrl } from "@celmic/protocol";
import type { Info } from "@/api";
import { Button } from "@/components/ui/button";
import { TapeCounter } from "@/components/tape-counter";
import { cn } from "@/lib/utils";

/** The QR code and the 6-digit code, printed like a label on card stock. */
export function PairCode({
  info,
  size,
  showCode = true,
  className,
}: {
  info: Info | null;
  size: "sm" | "lg";
  /** Off where the index stripe already shows the code. */
  showCode?: boolean;
  className?: string;
}) {
  const url = useMemo(
    () =>
      info && info.ips.length > 0
        ? buildConnectUrl({ hosts: info.ips.slice(0, 4), port: info.port, code: info.code, name: info.name })
        : null,
    [info],
  );
  const qr = size === "lg" ? 232 : 132;

  return (
    <div className={cn("flex items-center", size === "lg" ? "gap-10" : "gap-5", className)}>
      <div
        className="grid shrink-0 place-items-center rounded-md bg-foreground p-2.5"
        style={{ width: qr + 20, height: qr + 20 }}
      >
        {url ? (
          <QRCodeSVG value={url} size={qr} marginSize={0} level="M" bgColor="#eceae4" fgColor="#16171a" />
        ) : (
          <span className="px-3 text-center text-sm font-medium text-background">
            {info ? "No network connection" : "…"}
          </span>
        )}
      </div>
      <div className="flex min-w-0 flex-col gap-2">
        {showCode && (
          <TapeCounter
            value={info?.code ?? "000000"}
            label={`Pairing code ${info?.code.split("").join(" ") ?? ""}`}
            className={size === "lg" ? "text-5xl" : "text-[1.75rem]"}
          />
        )}
        <p className={cn("text-muted-foreground", size === "lg" ? "max-w-[34ch] text-lg" : "text-sm")}>
          Scan with the celmic app (or the camera), or pick <strong className="text-foreground">{info?.name ?? "this PC"}</strong>{" "}
          in the app and type the code.
        </p>
      </div>
    </div>
  );
}

export function PairPanel({
  info,
  rotatedForSafety,
  onRegenerate,
}: {
  info: Info | null;
  rotatedForSafety: boolean;
  onRegenerate: () => void;
}) {
  return (
    <div className="flex flex-col gap-8">
      {rotatedForSafety && (
        <p role="alert" className="rounded-md bg-ready px-4 py-3 font-medium text-ready-foreground">
          Someone typed too many wrong codes, so celmic changed it. Pair your phone again with the new code.
        </p>
      )}
      <PairCode info={info} size="lg" />
      <div className="flex flex-wrap items-center gap-x-10 gap-y-4 border-t pt-6">
        <Button variant="outline" onClick={onRegenerate}>
          <RefreshCw />
          New code
        </Button>
        <dl className="flex gap-10">
          <div>
            <dt className="text-sm text-muted-foreground">This PC</dt>
            <dd className="font-medium">{info?.name ?? "…"}</dd>
          </div>
          <div>
            <dt className="text-sm text-muted-foreground">Addresses</dt>
            <dd data-slot="figure" className="font-mono text-sm">
              {info?.ips.length ? info.ips.join("  ·  ") : "none"}
            </dd>
          </div>
          <div>
            <dt className="text-sm text-muted-foreground">UDP port</dt>
            <dd data-slot="figure" className="font-mono text-sm">
              {info?.port ?? "…"}
            </dd>
          </div>
        </dl>
      </div>
    </div>
  );
}
