# CelmicVAD — Windows virtual microphone driver

A root-enumerated WaveRT audio driver (PortCls + KMDF) that adds a capture endpoint named
**Celmic Microphone**. The Celmic desktop app feeds it the phone's audio, so any app
(Discord, Zoom, OBS, games) can pick it as a regular microphone.

Derived from Microsoft's [SimpleAudioSample](https://github.com/microsoft/Windows-driver-samples/tree/main/audio/simpleaudiosample)
(MS-PL, see [LICENSE-MS-PL.txt](LICENSE-MS-PL.txt)). The rest of Celmic is MIT.

## How it works

```
Celmic app ──DeviceIoControl──► \\.\CelmicVAD ──► 1 s ring buffer ──► WaveRT capture stream ──► apps
             (PCM s16 48 kHz)    control device     (spinlock)          drained at 48 kHz (QPC)
```

- `src/celmic.cpp` — control device (`\Device\CelmicVAD`, exclusive, any local user can
  read/write), the ring buffer, and the format conversion into the capture DMA buffer.
  PortCls owns the dispatch table, so the driver saves PortCls's routines and forwards every
  IRP that isn't for the control device. The control device is recognized by device type +
  a magic value in its extension, never by a global pointer.
- `src/celmic_ioctl.h` — the user/kernel contract (`IOCTL_CELMIC_PUSH`, `IOCTL_CELMIC_FLUSH`,
  `CELMIC_STATUS`). Mirrored in `apps/desktop/src-tauri/src/vad.rs`.
- `src/minwavertstream.cpp` — the sample's timer-driven capture stream, now reading from the
  ring instead of generating a sine wave; counts active capture streams.
- `src/adapter.cpp` — capture-only (the sample's fake speaker was removed); creates the control
  device on start and deletes it on removal so the driver can unload.

User mode keeps ~20 ms queued: it asks the driver how much is buffered every 4 ms and tops it
up. Driver and app share the system clock, so there is no drift on this hop; phone-vs-PC drift
is absorbed earlier by the app's resampler.

## Build

Needs Visual Studio 2022 with *Desktop development with C++*, the MSVC Spectre libraries and
the **Windows Driver Kit** component (`Component.Microsoft.Windows.DriverKit`). The WDK/SDK
themselves come from NuGet, pinned in `packages.config`.

```powershell
bun run driver:build                                   # x64 Release
.\drivers\windows\scripts\build.ps1 -Platform x64,ARM64 # ARM64 also needs the MSVC ARM64 tools
```

Output: `build\<platform>\Release\CelmicVAD\` (`CelmicVAD.sys`, `.inf`, `.cat`), signed with an
auto-generated test certificate. CI builds both architectures and uploads them as artifacts.

## Install (development)

Test-signed drivers only load in Windows **test mode** (Secure Boot off). Use a VM if you can.

```powershell
# elevated
.\drivers\windows\scripts\dev-setup.ps1 -EnableTestSigning   # trusts the test cert, enables test mode
# reboot, then in the Celmic app: Output > Install Celmic Microphone
```

The app installs it like `devcon install CelmicVAD.inf ROOT\CelmicVAD` (SetupAPI, through an
elevated copy of itself) and can remove it again from the same panel.

## Release signing

Shipping to regular users requires Microsoft's signature: an EV code-signing certificate, a
Partner Center hardware account, and attestation signing of the package (`.cab` with the
`.inf`/`.sys`). Then no test mode is needed.
