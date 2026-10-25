/**
 * Shared constants and helpers for the Celmic wire protocol.
 *
 * The audio path itself is implemented natively (Rust on desktop, Kotlin/Swift
 * on mobile) — see docs/PROTOCOL.md. This package only holds what the two
 * JavaScript UIs need to agree on: defaults and the pairing URL format.
 */

export const PROTOCOL_VERSION = 1;

/** Default UDP port the desktop listens on. */
export const DEFAULT_PORT = 47800;

/** DNS-SD service type advertised by the desktop app. */
export const SERVICE_TYPE = "_celmic._udp";

export const SAMPLE_RATE = 48_000;

/** Pairing codes are 6 decimal digits. */
export const PAIRING_CODE_LENGTH = 6;

export type ConnectTarget = {
  /** Candidate IPv4 addresses, in preference order. The phone races them. */
  hosts: string[];
  port: number;
  code: string;
  /** Human-readable desktop name, informational only. */
  name?: string;
};

const URL_SCHEME = "celmic:";
const URL_HOST = "connect";

export function isValidPairingCode(code: string): boolean {
  return new RegExp(`^\\d{${PAIRING_CODE_LENGTH}}$`).test(code);
}

/**
 * Builds the URL encoded in the desktop QR code, e.g.
 * `celmic://connect?h=192.168.0.10,10.0.0.4&p=47800&c=123456&n=Workstation`.
 *
 * Scanning it with the system camera deep-links into the mobile app.
 */
export function buildConnectUrl(target: ConnectTarget): string {
  const params = new URLSearchParams();
  params.set("h", target.hosts.join(","));
  params.set("p", String(target.port));
  params.set("c", target.code);
  if (target.name) params.set("n", target.name);
  params.set("v", String(PROTOCOL_VERSION));
  return `${URL_SCHEME}//${URL_HOST}?${params.toString()}`;
}

/** Parses a pairing URL. Returns `null` for anything that isn't a valid Celmic URL. */
export function parseConnectUrl(raw: string): ConnectTarget | null {
  const trimmed = raw.trim();
  if (!trimmed.toLowerCase().startsWith(`${URL_SCHEME}//${URL_HOST}`)) return null;

  const query = trimmed.slice(trimmed.indexOf("?") + 1);
  if (!trimmed.includes("?") || !query) return null;
  return parseConnectParams(new URLSearchParams(query));
}

/** Same as {@link parseConnectUrl} but from already-split query params (e.g. expo-router search params). */
export function parseConnectParams(
  params: URLSearchParams | Record<string, string | string[] | undefined>,
): ConnectTarget | null {
  const get = (key: string): string | undefined => {
    if (params instanceof URLSearchParams) return params.get(key) ?? undefined;
    const value = params[key];
    return Array.isArray(value) ? value[0] : value;
  };

  const hosts = (get("h") ?? "")
    .split(",")
    .map((h) => h.trim())
    .filter(isIPv4);
  const port = Number(get("p") ?? DEFAULT_PORT);
  const code = get("c") ?? "";
  const name = get("n");

  if (hosts.length === 0) return null;
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  if (!isValidPairingCode(code)) return null;

  return { hosts, port, code, ...(name ? { name } : {}) };
}

export function isIPv4(value: string): boolean {
  const parts = value.split(".");
  return (
    parts.length === 4 &&
    parts.every((p) => /^\d{1,3}$/.test(p) && Number(p) <= 255)
  );
}
