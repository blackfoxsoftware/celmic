# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Anyone who needs a microphone on their computer and has a phone in their pocket:

- gamers and streamers (Discord, OBS, Twitch) who want a decent mic without buying a headset;
- people on work calls (Zoom, Meet, Teams) from a laptop or desktop without a good microphone;
- creators recording podcasts, narration or video (Audition, OBS), where audio quality matters;
- anyone with a desktop PC and no microphone at all, who wants it working in a minute and then forgotten.

Typical scene: the user is about to join a call, start a stream or record, opens Celmic on the PC and on the phone, pairs them once, and then leaves both running in the background.

## Product Purpose

Celmic turns a phone into the computer's microphone over the local network. It is an open-source alternative to WO Mic: no ads, low latency, and the phone shows up in every app as a real microphone ("Celmic Microphone"). Success: pairing takes seconds, audio is clean and immediate, and the user never has to think about it again.

## Positioning

- Open source and free, with no ads.
- Audio never touches JavaScript: native capture on the phone, 10 ms UDP packets, a Rust jitter buffer with clock-drift compensation on the PC.
- On Windows, a real "Celmic Microphone" with no virtual cable and no test mode: Celmic serves a USB Audio Class 2.0 microphone over USB/IP, usbip-win2 (Microsoft-signed) attaches it, Windows' own USB audio driver does the rest.
- Pairing by QR code or by picking the PC from an automatic list and typing a 6-digit code; nobody else on the network can inject audio.

## Operating Context

- Two apps that are always used together: the desktop app (Tauri 2 + React, Windows/macOS/Linux) and the phone app (Expo / React Native, Android and iOS).
- Desktop: shows the pairing QR code and 6-digit code, the live stream (level, buffer, loss, underruns, uptime), the output / virtual microphone setup, and tuning (gain, latency). Often left open or minimized next to Discord, OBS or a call app.
- Phone: finds PCs on the LAN (mDNS) or scans the QR code, streams the mic with the screen off (foreground service on Android), offers mute and a voice processing toggle (noise suppression / echo cancellation vs raw).
- Both run on the same Wi-Fi/LAN; the PC may have several network adapters.

## Capabilities and Constraints

- Desktop UI is a Tauri webview (React + Vite); mobile UI is React Native (Expo SDK 58, expo-router). The mobile app uses one Celmic identity on Android and iOS, deliberately not each OS's native design language, shared with the desktop.
- Requested stack for the redesign: shadcn/ui with Tailwind on desktop, and a shadcn-compatible library on React Native.
- The pairing code changes every time the desktop app starts; the phone then has to pair again.
- Windows setup of Celmic Microphone needs one UAC prompt (installing usbip-win2); macOS uses BlackHole, Linux a PipeWire/PulseAudio virtual source.
- Undecided: localization (the UI is English today; the maintainer speaks Portuguese).

## Brand Commitments

- The name is written "celmic", in lowercase.
- Everything else (icon, colors, tone) is open to change.

## Evidence on Hand

- README.md, docs/PROTOCOL.md, drivers/windows/README.md.
- No user counts, testimonials, reviews, benchmarks or press exist. Do not fabricate any.

## Product Principles

- Set and forget: the app earns its place by disappearing once audio flows.
- Status at a glance: whether the phone is connected and sound is flowing must be readable from across the desk.
- One path, no jargon: pairing and microphone setup are single, guided steps; technical stats are there for whoever looks.
- Honest about the plumbing: when something needs a permission, a driver or a restart, say so plainly.
- One product on two screens: desktop and phone feel like halves of the same device.
