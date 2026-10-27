//! Audio output: device enumeration and the playout sink.
//!
//! Three kinds of sink:
//! - a cpal output stream (speakers, VB-CABLE, BlackHole, PipeWire sinks),
//! - on Windows, the USB microphone Celmic exports over USB/IP (`usbmic`), and
//! - on Windows, the Celmic virtual microphone driver, fed through an IOCTL.
//!
//! The sink lives on its own thread so we never depend on `Stream: Send`
//! across platforms; the rest of the app talks to it through a channel.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use serde::Serialize;

use crate::jitter::Playout;
use crate::server::Shared;

const PREFERRED_RATE: u32 = 48_000;

/// Pseudo device id of the Celmic virtual microphone driver.
pub const VIRTUAL_MIC_ID: &str = "celmic:virtual-mic";
/// Pseudo device id of the USB/IP microphone.
pub const USB_MIC_ID: &str = "celmic:usb-mic";

/// Substrings (lowercase) of device names that are virtual cables, i.e. outputs whose
/// other end shows up as a microphone for other apps.
/// Deliberately specific: generic "virtual" would also match sinks nobody
/// records from, like "NVIDIA Virtual Audio Device" or "Oculus Virtual Audio".
const VIRTUAL_HINTS: &[&str] = &[
    "cable input",           // VB-CABLE
    "vb-audio",              // VB-Audio Hi-Fi / Point / etc.
    "voicemeeter",
    "virtual audio cable",   // Muzychenko VAC ("Line 1 (Virtual Audio Cable)")
    "blackhole",
    "soundflower",
    "loopback audio",        // Rogue Amoeba Loopback
    "celmic",
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_virtual: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveOutput {
    pub id: String,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub is_virtual: bool,
}

/// Live state of the virtual microphone driver while it is the active sink.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualMicInfo {
    /// Apps currently recording from "Celmic Microphone".
    pub active_streams: u32,
    pub queued_ms: f32,
    pub underruns: u32,
}

fn host() -> cpal::Host {
    // On Linux the PulseAudio host (also served by PipeWire) exposes real sinks,
    // including the "celmic" null sink; plain ALSA only shows "default"/"pipewire".
    #[cfg(target_os = "linux")]
    if let Ok(host) = cpal::host_from_id(cpal::HostId::PulseAudio) {
        return host;
    }
    cpal::default_host()
}

fn is_virtual(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIRTUAL_HINTS.iter().any(|hint| lower.contains(hint))
}

#[cfg(test)]
mod tests {
    use super::is_virtual;

    #[test]
    fn recognizes_real_virtual_cables_only() {
        assert!(is_virtual("CABLE Input (VB-Audio Virtual Cable)"));
        assert!(is_virtual("Line 1 (Virtual Audio Cable)"));
        assert!(is_virtual("BlackHole 2ch"));
        assert!(is_virtual("Celmic"));
        assert!(!is_virtual("NVIDIA Virtual Audio Device (Wave Extensible) (WDM)"));
        assert!(!is_virtual("Oculus Virtual Audio Device"));
        assert!(!is_virtual("Speakers (Realtek(R) Audio)"));
    }
}

fn describe(device: &cpal::Device) -> Option<(String, String)> {
    let id = device.id().ok()?.to_string();
    let name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| id.clone());
    Some((id, name))
}

/// Whether the Celmic driver is installed (it may be busy with another client).
pub fn virtual_mic_installed() -> bool {
    #[cfg(windows)]
    {
        !matches!(crate::vad::VirtualMic::presence(), crate::vad::Presence::Missing)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Whether Windows has the USB/IP microphone attached.
pub fn usb_mic_attached() -> bool {
    #[cfg(windows)]
    {
        crate::usbmic::get().is_some_and(|mic| mic.attached())
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn list_devices() -> Vec<DeviceInfo> {
    let mut list = Vec::new();
    if usb_mic_attached() {
        list.push(DeviceInfo {
            id: USB_MIC_ID.into(),
            name: "Celmic Microphone".into(),
            is_default: false,
            is_virtual: true,
        });
    }
    if virtual_mic_installed() {
        list.push(DeviceInfo {
            id: VIRTUAL_MIC_ID.into(),
            name: "Celmic Microphone (driver)".into(),
            is_default: false,
            is_virtual: true,
        });
    }

    let host = host();
    let default_id = host
        .default_output_device()
        .and_then(|d| d.id().ok())
        .map(|id| id.to_string());
    if let Ok(devices) = host.output_devices() {
        let mut outputs: Vec<DeviceInfo> = devices
            .filter_map(|d| describe(&d))
            .map(|(id, name)| DeviceInfo {
                is_default: default_id.as_deref() == Some(id.as_str()),
                is_virtual: is_virtual(&name),
                id,
                name,
            })
            .collect();
        outputs.sort_by(|a, b| b.is_virtual.cmp(&a.is_virtual).then_with(|| a.name.cmp(&b.name)));
        list.extend(outputs);
    }
    list
}

/// The cpal device we pick when the user hasn't chosen one: a virtual cable if present.
fn auto_device(host: &cpal::Host) -> Option<cpal::Device> {
    host.output_devices()
        .ok()
        .and_then(|mut devices| {
            devices.find(|d| describe(d).is_some_and(|(_, name)| is_virtual(&name)))
        })
        .or_else(|| host.default_output_device())
}

enum Command {
    Select {
        id: Option<String>,
        reply: Sender<Result<ActiveOutput, String>>,
    },
    /// Closes the current sink (e.g. so the driver can be uninstalled).
    Release { reply: Sender<()> },
}

/// Keeps whatever is playing alive; dropping it stops playback.
enum Sink {
    // Held only to keep the stream running.
    #[allow(dead_code)]
    Cpal(cpal::Stream),
    #[cfg(windows)]
    #[allow(dead_code)]
    Driver(driver_sink::DriverSink),
    #[cfg(windows)]
    #[allow(dead_code)]
    Usb(UsbSink),
}

/// Routes the phone into the USB/IP microphone while alive.
#[cfg(windows)]
struct UsbSink(&'static crate::usbmic::UsbMic);

#[cfg(windows)]
impl Drop for UsbSink {
    fn drop(&mut self) {
        self.0.set_feeding(false);
    }
}

#[derive(Clone)]
struct Slots {
    active: Arc<Mutex<Option<ActiveOutput>>>,
    error: Arc<Mutex<Option<String>>>,
    virtual_mic: Arc<Mutex<Option<VirtualMicInfo>>>,
}

pub struct Output {
    tx: Sender<Command>,
    slots: Slots,
    #[cfg_attr(not(windows), allow(dead_code))]
    shared: Arc<Shared>,
}

/// In automatic mode, whether the best sink is no longer the open one: the
/// USB/IP microphone was attached, or went away.
fn automatic_choice_changed(active: Option<&ActiveOutput>) -> bool {
    let on_usb = active.is_some_and(|a| a.id == USB_MIC_ID);
    let on_driver = active.is_some_and(|a| a.id == VIRTUAL_MIC_ID);
    !on_driver && on_usb != usb_mic_attached()
}

impl Output {
    pub fn spawn(shared: Arc<Shared>) -> Self {
        let (tx, rx) = mpsc::channel::<Command>();
        let slots = Slots {
            active: Arc::new(Mutex::new(None)),
            error: Arc::new(Mutex::new(None)),
            virtual_mic: Arc::new(Mutex::new(None)),
        };

        let thread_slots = slots.clone();
        let thread_shared = shared.clone();
        thread::Builder::new()
            .name("celmic-output".into())
            .spawn(move || {
                let (slots, shared) = (thread_slots, thread_shared);
                let mut sink: Option<Sink> = None;
                // None = automatic selection.
                let mut selected: Option<String> = None;
                let open = |id: &Option<String>, sink: &mut Option<Sink>| {
                    *sink = None; // release the old device before opening again
                    *slots.virtual_mic.lock().unwrap() = None;
                    match open_sink(id.as_deref(), &shared, &slots) {
                        Ok((s, info)) => {
                            *sink = Some(s);
                            *slots.active.lock().unwrap() = Some(info.clone());
                            *slots.error.lock().unwrap() = None;
                            Ok(info)
                        }
                        Err(e) => {
                            *slots.active.lock().unwrap() = None;
                            *slots.error.lock().unwrap() = Some(e.clone());
                            Err(e)
                        }
                    }
                };

                let _ = open(&selected, &mut sink);

                loop {
                    match rx.recv_timeout(Duration::from_secs(2)) {
                        Ok(Command::Select { id, reply }) => {
                            selected = id;
                            let _ = reply.send(open(&selected, &mut sink));
                        }
                        Ok(Command::Release { reply }) => {
                            sink = None;
                            *slots.active.lock().unwrap() = None;
                            *slots.virtual_mic.lock().unwrap() = None;
                            *slots.error.lock().unwrap() = None;
                            let _ = reply.send(());
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            // Device unplugged / audio service restarted: try to come back.
                            let failed = slots.error.lock().unwrap().is_some();
                            let better = selected.is_none()
                                && automatic_choice_changed(slots.active.lock().unwrap().as_ref());
                            if failed || better {
                                let _ = open(&selected, &mut sink);
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn output thread");

        Self { tx, slots, shared }
    }

    pub fn select(&self, id: Option<String>) -> Result<ActiveOutput, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::Select { id, reply })
            .map_err(|_| "output thread is gone".to_string())?;
        rx.recv().map_err(|_| "output thread is gone".to_string())?
    }

    /// Stops playback and closes the device until the next `select`.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn release(&self) {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(Command::Release { reply }).is_ok() {
            let _ = rx.recv();
        }
    }

    pub fn active(&self) -> Option<ActiveOutput> {
        self.slots.active.lock().unwrap().clone()
    }

    pub fn error(&self) -> Option<String> {
        self.slots.error.lock().unwrap().clone()
    }

    pub fn virtual_mic(&self) -> Option<VirtualMicInfo> {
        #[cfg(windows)]
        if self.active().is_some_and(|a| a.id == USB_MIC_ID) {
            let mic = crate::usbmic::get()?;
            return Some(VirtualMicInfo {
                active_streams: mic.streaming() as u32,
                queued_ms: self.shared.jitter.fill_ms(),
                underruns: self.shared.jitter.underruns() as u32,
            });
        }
        self.slots.virtual_mic.lock().unwrap().clone()
    }
}

fn open_sink(id: Option<&str>, shared: &Arc<Shared>, slots: &Slots) -> Result<(Sink, ActiveOutput), String> {
    #[cfg(windows)]
    {
        let wants_driver = match id {
            Some(id) => id == VIRTUAL_MIC_ID,
            // Automatic: the native driver beats any third-party cable.
            None => virtual_mic_installed(),
        };
        if wants_driver {
            match driver_sink::open(shared, slots) {
                Ok(opened) => return Ok(opened),
                Err(e) if id.is_some() => return Err(e),
                Err(_) => {} // automatic mode: fall back to the next choice
            }
        }
        // Automatic: then the USB/IP microphone, before any third-party cable.
        if id == Some(USB_MIC_ID) || (id.is_none() && usb_mic_attached()) {
            let mic = crate::usbmic::get().ok_or("Celmic Microphone is not available")?;
            mic.set_feeding(true);
            let active = ActiveOutput {
                id: USB_MIC_ID.into(),
                name: "Celmic Microphone".into(),
                sample_rate: 48_000,
                channels: 2,
                is_virtual: true,
            };
            return Ok((Sink::Usb(UsbSink(mic)), active));
        }
    }
    let (stream, info) = open_stream(id, shared, &slots.error)?;
    Ok((Sink::Cpal(stream), info))
}

fn open_stream(
    id: Option<&str>,
    shared: &Arc<Shared>,
    error_slot: &Arc<Mutex<Option<String>>>,
) -> Result<(cpal::Stream, ActiveOutput), String> {
    let host = host();
    let device = match id {
        Some(id) => {
            let device_id = id.parse::<cpal::DeviceId>().map_err(|e| e.to_string())?;
            host.device_by_id(&device_id)
                .ok_or_else(|| format!("output device not found: {id}"))?
        }
        None => auto_device(&host).ok_or("no audio output device available")?,
    };
    let (device_id, name) = describe(&device).ok_or("could not read device id")?;

    let default = device.default_output_config().map_err(|e| e.to_string())?;
    let format = default.sample_format();
    // Prefer 48 kHz (the phone's rate) to skip resampling when the device allows it.
    let supported = device
        .supported_output_configs()
        .ok()
        .and_then(|mut configs| {
            configs.find(|c| {
                c.sample_format() == format
                    && c.channels() == default.channels()
                    && c.min_sample_rate() <= PREFERRED_RATE
                    && c.max_sample_rate() >= PREFERRED_RATE
            })
        })
        .map(|c| c.with_sample_rate(PREFERRED_RATE))
        .unwrap_or(default);

    let config: StreamConfig = supported.into();
    let info = ActiveOutput {
        id: device_id,
        is_virtual: is_virtual(&name),
        name,
        sample_rate: config.sample_rate,
        channels: config.channels,
    };

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, shared, error_slot),
        SampleFormat::I16 => build::<i16>(&device, config, shared, error_slot),
        SampleFormat::I32 => build::<i32>(&device, config, shared, error_slot),
        SampleFormat::U16 => build::<u16>(&device, config, shared, error_slot),
        SampleFormat::F64 => build::<f64>(&device, config, shared, error_slot),
        other => Err(format!("unsupported sample format {other}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, info))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: &Arc<Shared>,
    error_slot: &Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut playout = Playout::new(config.sample_rate);
    let mut mono: Vec<f32> = Vec::with_capacity(4096);
    let shared = shared.clone();
    let error_slot = error_slot.clone();

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels;
                mono.resize(frames, 0.0);
                let target = shared.target_latency_ms.load(Ordering::Relaxed);
                playout.fill(&shared.jitter, target, &mut mono);
                for (frame, &sample) in data.chunks_mut(channels).zip(mono.iter()) {
                    frame.fill(T::from_sample(sample));
                }
            },
            move |err: cpal::Error| {
                use cpal::ErrorKind;
                match err.kind() {
                    // Transient / informational: keep the stream.
                    ErrorKind::Xrun | ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
                    _ => *error_slot.lock().unwrap() = Some(err.to_string()),
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
mod driver_sink {
    //! Feeds the Celmic virtual microphone driver.
    //!
    //! The driver drains its queue at exactly 48 kHz of system (QPC) time, so
    //! we simply top the queue up to a small target every few milliseconds.
    //! Phone-vs-PC clock drift is handled upstream by `Playout`.

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use super::{ActiveOutput, Sink, Slots, VIRTUAL_MIC_ID, VirtualMicInfo};
    use crate::jitter::Playout;
    use crate::server::Shared;
    use crate::vad::{DriverStatus, SAMPLE_RATE, VirtualMic};

    /// Audio kept queued in the driver: enough to ride out scheduler hiccups.
    const QUEUE_TARGET_FRAMES: u32 = SAMPLE_RATE / 50; // 20 ms
    const TICK: Duration = Duration::from_millis(4);
    const MAX_CHUNK: usize = (SAMPLE_RATE / 10) as usize; // 100 ms

    pub struct DriverSink {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for DriverSink {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn info(status: &DriverStatus) -> VirtualMicInfo {
        VirtualMicInfo {
            active_streams: status.active_streams,
            queued_ms: status.queued_frames as f32 * 1000.0 / SAMPLE_RATE as f32,
            underruns: status.underruns,
        }
    }

    pub fn open(shared: &Arc<Shared>, slots: &Slots) -> Result<(Sink, ActiveOutput), String> {
        let mic = VirtualMic::open().map_err(|e| format!("Celmic virtual microphone unavailable: {e}"))?;
        let stop = Arc::new(AtomicBool::new(false));

        let (thread_stop, shared, error_slot, vm_slot) =
            (stop.clone(), shared.clone(), slots.error.clone(), slots.virtual_mic.clone());
        let thread = thread::Builder::new()
            .name("celmic-virtual-mic".into())
            .spawn(move || feed(mic, &shared, &thread_stop, &error_slot, &vm_slot))
            .map_err(|e| e.to_string())?;

        let sink = DriverSink { stop, thread: Some(thread) };
        let active = ActiveOutput {
            id: VIRTUAL_MIC_ID.into(),
            name: "Celmic Microphone (driver)".into(),
            sample_rate: SAMPLE_RATE,
            channels: 1,
            is_virtual: true,
        };
        Ok((Sink::Driver(sink), active))
    }

    fn feed(
        mic: VirtualMic,
        shared: &Shared,
        stop: &AtomicBool,
        error_slot: &Mutex<Option<String>>,
        vm_slot: &Mutex<Option<VirtualMicInfo>>,
    ) {
        let mut playout = Playout::new(SAMPLE_RATE);
        let mut mono = vec![0f32; MAX_CHUNK];
        let mut pcm = vec![0i16; MAX_CHUNK];
        let mut published = Instant::now();

        while !stop.load(Ordering::Relaxed) {
            let result = mic.push(&[]).and_then(|status| {
                let need = QUEUE_TARGET_FRAMES.saturating_sub(status.queued_frames) as usize;
                if need == 0 {
                    return Ok(status);
                }
                let n = need.min(MAX_CHUNK);
                let target = shared.target_latency_ms.load(Ordering::Relaxed);
                playout.fill(&shared.jitter, target, &mut mono[..n]);
                for (out, &sample) in pcm.iter_mut().zip(&mono[..n]) {
                    *out = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                }
                mic.push(&pcm[..n])
            });

            match result {
                Ok(status) => {
                    if published.elapsed() >= Duration::from_millis(100) {
                        published = Instant::now();
                        *vm_slot.lock().unwrap() = Some(info(&status));
                    }
                }
                Err(e) => {
                    // The output thread notices the error and reopens the sink.
                    *error_slot.lock().unwrap() = Some(format!("virtual microphone: {e}"));
                    break;
                }
            }
            thread::sleep(TICK);
        }

        let _ = mic.flush();
        *vm_slot.lock().unwrap() = None;
    }
}
