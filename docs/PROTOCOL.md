# Celmic wire protocol — v1

Everything travels over **UDP** (default port `47800`) between one phone ("client")
and one desktop ("server"). The audio path never touches JavaScript on either side.

## Discovery

The desktop advertises a DNS-SD service:

| field    | value                                                          |
| -------- | -------------------------------------------------------------- |
| type     | `_celmic._udp.local.`                                          |
| instance | desktop hostname                                               |
| port     | UDP port the server listens on                                 |
| TXT `v`  | protocol version (`1`)                                         |
| TXT `name` | human readable desktop name                                  |
| TXT `port` | UDP port (duplicated from SRV for clients that only read TXT) |
| TXT `ips` | comma-separated IPv4 candidates, preferred first (used on iOS, where resolving is awkward) |

Discovery never reveals the pairing code. Pairing happens either by scanning the
desktop QR code (`celmic://connect?h=<ips>&p=<port>&c=<code>&n=<name>&v=1`) or by
typing the 6-digit code shown on the desktop.

## Packet layout

Every packet starts with a fixed 20-byte header. Multi-byte header fields are **big-endian**.

```
offset size field
0      4    magic        "CMIC" (0x43 0x4D 0x49 0x43)
4      1    version      1
5      1    type         see below
6      1    codec        AUDIO only: 0 = PCM signed 16-bit little-endian
7      1    channels     AUDIO only: 1
8      4    session      0 in HELLO, otherwise the id assigned by WELCOME
12     4    seq          AUDIO: monotonically increasing per session (wraps)
16     4    sample_rate  AUDIO only, in Hz (usually 48000)
20     ...  payload
```

| type   | name    | direction        | payload                                                     |
| ------ | ------- | ---------------- | ----------------------------------------------------------- |
| `0x01` | HELLO   | phone → desktop  | 6 ASCII digits (pairing code) followed by UTF-8 device name (≤ 64 bytes) |
| `0x02` | WELCOME | desktop → phone  | UTF-8 desktop name; header `session` carries the new id     |
| `0x03` | REJECT  | desktop → phone  | 1 byte reason: `1` bad code, `2` unknown session, `3` unsupported version, `4` kicked by the user, `5` replaced by another phone |
| `0x10` | AUDIO   | phone → desktop  | PCM frames (10 ms = 480 samples @ 48 kHz is the default)    |
| `0x20` | PING    | phone → desktop  | 8 opaque bytes (phone timestamp)                            |
| `0x21` | PONG    | desktop → phone  | the PING payload, echoed                                    |
| `0x30` | BYE     | phone → desktop  | empty                                                       |

## Session lifecycle

1. The phone sends HELLO to every candidate address every 300 ms until one answers
   (racing addresses solves multi-NIC desktops: Wi-Fi + Ethernet + WSL/Docker adapters).
2. The desktop checks the code. On success it answers WELCOME with a random non-zero
   session id. A session is identified by **(phone IP, session id)**, not by the full
   socket address: a repeated HELLO from the same IP — even from another source port, as
   iOS uses one socket per candidate address — gets the **same** session id back, and
   AUDIO/PING from any port of that IP is accepted (replies go to the latest port).
3. The phone streams AUDIO and sends a PING every second (RTT shown in the app).
   `sample_rate` must be within 8 000–192 000 Hz or the packet is ignored.
4. A newer HELLO with the right code from **another IP** takes over (last phone wins).
   The previous phone is told `REJECT(5)` and stops instead of fighting back.
5. When the user presses Disconnect on the desktop, the phone gets `REJECT(4)` and stops.
   Reasons 4 and 5 are remembered for 60 s per (IP, session).
6. AUDIO/PING with an otherwise unknown session gets `REJECT(2)`; the phone then re-sends
   HELLO automatically, which makes desktop restarts transparent.
7. The desktop drops a session after 5 s without packets. BYE ends it immediately.

### Pairing-code protection

- After a wrong code, HELLOs from that IP are ignored for 1 s.
- After 10 wrong codes the desktop generates a new code (the UI tells the user). With
  6 digits that caps an attacker at a 10⁻⁵ chance per code; the worst they can do is
  force a re-pair.
- REJECTs are rate-limited per destination address (one per 500 ms).

## Desktop playout

Incoming PCM goes into an adaptive jitter buffer (target latency configurable, default 40 ms).
The output callback resamples from the phone's rate to the output device's rate and nudges the
ratio by up to ±0.3 % to keep the buffer at its target, which absorbs clock drift between
phone and PC without periodic clicks. Underruns re-buffer to the target; overruns drop the
oldest audio down to the target.
