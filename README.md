# celmic

Use your phone as a microphone for your computer — open source, no ads, low latency.

A WO Mic alternative built with **Tauri** (desktop) and **Expo / React Native** (phone),
in a single **Bun** monorepo.

## Why it's better

| | celmic |
| --- | --- |
| **Latency** | Audio never touches JavaScript: Kotlin `AudioRecord` / Swift `AVAudioSinkNode` → 10 ms UDP packets → Rust jitter buffer → `cpal`. ~40 ms buffer by default, adjustable. |
| **No clicks over time** | The desktop resamples continuously and nudges the playback rate (±0.3 %) to absorb the clock drift between phone and PC. |
| **Pairing** | Scan a QR code (in-app or with the system camera, via `celmic://` deep link), or pick the PC from an automatic list (mDNS) and type a 6-digit code. Nobody else on the network can inject audio. |
| **Multi-NIC friendly** | The phone races every IP the PC advertises (Wi-Fi, Ethernet, WSL/Docker adapters) and keeps whichever answers first. |
| **Resilient** | Auto-reconnects when the PC app restarts or Wi-Fi blips. Android runs a microphone foreground service + low-latency Wi-Fi lock, so it keeps streaming with the screen off. |
| **Voice or raw** | Toggle noise suppression / echo cancellation, or stream the unprocessed mic. |
| **Native Windows mic** | An open-source kernel audio driver adds a real **Celmic Microphone** device — no VB-CABLE needed. |
| **Open protocol** | Documented in [docs/PROTOCOL.md](docs/PROTOCOL.md). |

## Layout

```
apps/
  desktop/                 Tauri 2 + React (Vite)
    src-tauri/src/
      server.rs            UDP handshake, sessions, PCM ingestion
      jitter.rs            adaptive jitter buffer + drift-compensating resampler
      output.rs            cpal output device + stream thread
      discovery.rs         LAN IPs + DNS-SD advertisement (_celmic._udp)
      virtual_mic.rs       one-click virtual mic on Linux (PipeWire/PulseAudio)
      vad.rs               client for the Windows driver (\.CelmicVAD)
      driver_install.rs    installs the driver (SetupAPI, elevated helper)
  mobile/                  Expo SDK 58 + expo-router
    modules/celmic-audio/  local native module (Kotlin + Swift): capture, UDP, discovery
packages/
  protocol/                shared constants + pairing URL helpers (TS)
drivers/
  windows/                 CelmicVAD: WaveRT virtual microphone (KMDF/PortCls, MS-PL)
docs/PROTOCOL.md           wire format
```

## Requirements

- [Bun](https://bun.sh) ≥ 1.3
- Desktop: [Rust](https://rustup.rs) + the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS
- Mobile: Android Studio (Android SDK + JDK 17) and/or Xcode on macOS. The app uses a custom native module, so it needs a **development build** — Expo Go won't work.

## Getting started

```bash
bun install

# desktop
bun run desktop          # tauri dev

# phone (USB-connected device or emulator)
bun run mobile:android   # expo run:android — builds the native app and starts Metro
bun run mobile:ios       # expo run:ios (macOS)
```

Without a local Android/iOS toolchain you can build in the cloud with EAS:
`cd apps/mobile && bunx eas-cli build --profile development`.

## Making it show up as a microphone

The desktop app plays the phone's audio into an output device. To have other apps
(Discord, Zoom, OBS, games) see it as a **microphone**, route it into a virtual cable:

| OS | How |
| --- | --- |
| Windows | Click **Install Celmic Microphone** in the app (native driver, see [drivers/windows](drivers/windows/README.md)), then pick **Celmic Microphone** as the input in your apps. [VB-CABLE](https://vb-audio.com/Cable/) also works. |
| macOS | Install [BlackHole 2ch](https://existential.audio/blackhole/). Celmic auto-selects it; pick BlackHole as the mic. |
| Linux | Click **Create virtual mic** in the app (creates a `celmic` null sink + `Celmic_Microphone` source via `pactl`). |

A native macOS virtual microphone (Core Audio plug-in) is on the roadmap.

> **Driver signing:** builds from source are test-signed and only load with Windows test mode
> (see [drivers/windows](drivers/windows/README.md)). Signed release builds will need none of that.

## Development

```bash
bun run typecheck                          # all workspaces
bun test packages/protocol                 # protocol helpers
cd apps/desktop/src-tauri && cargo test    # protocol, sessions, jitter buffer, IP ranking
bun run driver:build                       # Windows driver package (needs VS 2022 + WDK component)
```

Environment variable `CELMIC_PORT` changes the desktop's UDP port (default `47800`).

## Roadmap

- [ ] Opus codec (lower bandwidth, better on weak Wi-Fi) with FEC
- [ ] USB mode (Android `adb reverse` over TCP)
- [x] Native virtual microphone driver (Windows)
- [ ] Native virtual microphone (macOS AudioServerPlugIn)
- [ ] Microsoft-signed driver releases (attestation signing)
- [ ] Tray icon + start minimized, remember output/gain/latency
- [ ] Several phones at once (multi-mic / stereo pair)
- [ ] Phone-side gain, mic selection (front/back/bottom), Bluetooth mics

## License

[MIT](LICENSE)
