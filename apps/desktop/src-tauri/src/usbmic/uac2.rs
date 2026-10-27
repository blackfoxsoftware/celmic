//! A full-speed USB Audio Class 2.0 microphone: descriptors, control requests
//! and the capture endpoint, fed from the phone's jitter buffer.
//!
//! UAC2 (not UAC1) so that Windows binds usbaudio2.sys, whose WaveRT pipeline
//! keeps streaming under load; usbaudio.sys resets the pipe when the audio
//! engine falls behind.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::jitter::Playout;
use crate::server::Shared;

pub const SAMPLE_RATE: u32 = 48_000;
/// Stereo (the phone's mono audio on both channels), 24-bit.
const CHANNELS: usize = 2;
const SUBSLOT_BYTES: usize = 3;
pub const FRAME_BYTES: usize = CHANNELS * SUBSLOT_BYTES;
/// One packet per 1 ms full-speed frame.
pub const PACKET_FRAMES: usize = SAMPLE_RATE as usize / 1000;
pub const PACKET_BYTES: usize = PACKET_FRAMES * FRAME_BYTES;

/// The isochronous IN endpoint (0x81).
pub const CAPTURE_ENDPOINT: u32 = 1;
pub const INTERFACES: u8 = 2;

/// Unregistered "experimental" vendor ID, as used by local virtual devices.
pub const VENDOR_ID: u16 = 0xFFFF;
pub const PRODUCT_ID: u16 = 0xC31C;
pub const PRODUCT: &str = "Celmic Microphone";

const DESCRIPTOR_DEVICE: u8 = 0x01;
const DESCRIPTOR_CONFIGURATION: u8 = 0x02;
const DESCRIPTOR_STRING: u8 = 0x03;
const DESCRIPTOR_INTERFACE: u8 = 0x04;
const DESCRIPTOR_ENDPOINT: u8 = 0x05;
const DESCRIPTOR_INTERFACE_ASSOCIATION: u8 = 0x0B;
const CS_INTERFACE: u8 = 0x24;
const CS_ENDPOINT: u8 = 0x25;

/// Audio interface protocol: USB Audio 2.0.
const IP_VERSION_02_00: u8 = 0x20;

/// Topology: microphone input terminal -> feature unit -> USB streaming
/// output terminal, all on one fixed clock.
const CLOCK_SOURCE: u8 = 9;
const INPUT_TERMINAL: u8 = 1;
const FEATURE_UNIT: u8 = 2;
const OUTPUT_TERMINAL: u8 = 3;

const REQUEST_GET_STATUS: u8 = 0x00;
pub const REQUEST_CLEAR_FEATURE: u8 = 0x01;
const REQUEST_SET_FEATURE: u8 = 0x03;
const REQUEST_SET_ADDRESS: u8 = 0x05;
const REQUEST_GET_DESCRIPTOR: u8 = 0x06;
const REQUEST_GET_CONFIGURATION: u8 = 0x08;
pub const REQUEST_SET_CONFIGURATION: u8 = 0x09;
const REQUEST_GET_INTERFACE: u8 = 0x0A;
pub const REQUEST_SET_INTERFACE: u8 = 0x0B;
const REQUEST_SYNCH_FRAME: u8 = 0x0C;

/// UAC2 class requests (ADC-2 5.2.2).
const AUDIO_CUR: u8 = 0x01;
const AUDIO_RANGE: u8 = 0x02;
const CS_SAM_FREQ_CONTROL: u8 = 0x01;
const CS_CLOCK_VALID_CONTROL: u8 = 0x02;
const FU_MUTE_CONTROL: u8 = 0x01;
const FU_VOLUME_CONTROL: u8 = 0x02;

/// Feature unit volume range, in 1/256 dB.
const VOLUME_MIN: i16 = -60 * 256;
const VOLUME_MAX: i16 = 0;
const VOLUME_RES: i16 = 256;
/// Minus infinity dB.
const VOLUME_SILENT: i16 = i16::MIN;

/// EPIPE: the request is stalled.
pub const STALL: i32 = -32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetupPacket {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl SetupPacket {
    pub fn parse(b: &[u8; 8]) -> SetupPacket {
        SetupPacket {
            request_type: b[0],
            request: b[1],
            value: u16::from_le_bytes([b[2], b[3]]),
            index: u16::from_le_bytes([b[4], b[5]]),
            length: u16::from_le_bytes([b[6], b[7]]),
        }
    }

    /// CLEAR_FEATURE(ENDPOINT_HALT): the host has reset a pipe.
    pub fn is_endpoint_halt_clear(&self) -> bool {
        self.request_type == 0x02 && self.request == REQUEST_CLEAR_FEATURE && self.value == 0
    }

    /// Requests that stop or restart the audio stream.
    pub fn restarts_streams(&self) -> bool {
        (self.request_type == 0x01 && self.request == REQUEST_SET_INTERFACE)
            || (self.request_type == 0x00 && self.request == REQUEST_SET_CONFIGURATION)
    }
}

pub fn device_descriptor() -> Vec<u8> {
    let mut b = vec![18, DESCRIPTOR_DEVICE];
    // USB 2.0, full speed only: no device qualifier, no BOS descriptor.
    b.extend_from_slice(&0x0200u16.to_le_bytes());
    // Miscellaneous / common class / interface association: UAC2 groups the
    // audio function's interfaces with an IAD.
    b.extend_from_slice(&[0xEF, 0x02, 0x01, 64]);
    b.extend_from_slice(&VENDOR_ID.to_le_bytes());
    b.extend_from_slice(&PRODUCT_ID.to_le_bytes());
    b.extend_from_slice(&0x0100u16.to_le_bytes()); // bcdDevice
    b.extend_from_slice(&[1, 2, 3, 1]); // manufacturer, product, serial strings; 1 configuration
    b
}

pub fn configuration_descriptor() -> Vec<u8> {
    let c = CS_INTERFACE;
    let le16 = |v: u16| v.to_le_bytes();
    let channels = 0x0000_0003u32.to_le_bytes(); // front left, front right
    let mut b: Vec<u8> = Vec::new();

    // Configuration header; wTotalLength is patched at the end. Bus powered, 100 mA.
    b.extend_from_slice(&[9, DESCRIPTOR_CONFIGURATION, 0, 0, INTERFACES, 1, 0, 0x80, 50]);
    // Interface association: interfaces 0-1 are one audio function.
    b.extend_from_slice(&[8, DESCRIPTOR_INTERFACE_ASSOCIATION, 0, INTERFACES, 0x01, 0x00, IP_VERSION_02_00, 0]);

    // AudioControl interface 0.
    b.extend_from_slice(&[9, DESCRIPTOR_INTERFACE, 0, 0, 0, 0x01, 0x01, IP_VERSION_02_00, 0]);
    let header = b.len();
    // AC header (ADC-2 4.7.2): ADC 2.0, category microphone; wTotalLength patched below.
    b.extend_from_slice(&[9, c, 0x01, 0x00, 0x02, 0x03, 0, 0, 0x00]);
    // Clock source (4.7.2.1): internal fixed clock synchronized to SOF, with
    // read-only frequency and validity controls.
    b.extend_from_slice(&[8, c, 0x0A, CLOCK_SOURCE, 0x05, 0x05, 0, 0]);
    // Microphone input terminal (4.7.2.4).
    b.extend_from_slice(&[17, c, 0x02, INPUT_TERMINAL]);
    b.extend_from_slice(&le16(0x0201));
    b.extend_from_slice(&[0, CLOCK_SOURCE, CHANNELS as u8]);
    b.extend_from_slice(&channels);
    b.extend_from_slice(&[0, 0, 0, 0]);
    // Feature unit (4.7.2.8): mute and volume on the master channel only.
    b.extend_from_slice(&[18, c, 0x06, FEATURE_UNIT, INPUT_TERMINAL]);
    b.extend_from_slice(&0x0000_000Fu32.to_le_bytes());
    b.extend_from_slice(&[0; 8]); // left and right: no controls
    b.push(0);
    // USB streaming output terminal (4.7.2.5).
    b.extend_from_slice(&[12, c, 0x03, OUTPUT_TERMINAL]);
    b.extend_from_slice(&le16(0x0101));
    b.extend_from_slice(&[0, FEATURE_UNIT, CLOCK_SOURCE, 0, 0, 0]);
    let ac_length = (b.len() - header) as u16;
    b[header + 6..header + 8].copy_from_slice(&le16(ac_length));

    // AudioStreaming interface 1: alternate 0 is idle, alternate 1 streams.
    b.extend_from_slice(&[9, DESCRIPTOR_INTERFACE, 1, 0, 0, 0x01, 0x02, IP_VERSION_02_00, 0]);
    b.extend_from_slice(&[9, DESCRIPTOR_INTERFACE, 1, 1, 1, 0x01, 0x02, IP_VERSION_02_00, 0]);
    // AS general (4.9.2): Type I PCM.
    b.extend_from_slice(&[16, c, 0x01, OUTPUT_TERMINAL, 0x00, 0x01]);
    b.extend_from_slice(&0x0000_0001u32.to_le_bytes());
    b.push(CHANNELS as u8);
    b.extend_from_slice(&channels);
    b.push(0);
    // Type I format (FMT-2 2.3.1.6): 3-byte subslots, 24 bits.
    b.extend_from_slice(&[6, c, 0x02, 0x01, SUBSLOT_BYTES as u8, 24]);
    // Isochronous IN endpoint, synchronous to the bus frame clock: exactly one
    // 48-frame packet per frame, so no feedback endpoint.
    b.extend_from_slice(&[7, DESCRIPTOR_ENDPOINT, 0x80 | CAPTURE_ENDPOINT as u8, 0x0D]);
    b.extend_from_slice(&le16(PACKET_BYTES as u16));
    b.push(1);
    // Class-specific endpoint (4.10.1.2).
    b.extend_from_slice(&[8, CS_ENDPOINT, 0x01, 0x00, 0x00, 0x00, 0, 0]);

    let total = b.len() as u16;
    b[2..4].copy_from_slice(&total.to_le_bytes());
    b
}

fn string_descriptor(s: &str) -> Vec<u8> {
    let mut b = vec![0, DESCRIPTOR_STRING];
    for unit in s.encode_utf16().take(126) {
        b.extend_from_slice(&unit.to_le_bytes());
    }
    b[0] = b.len() as u8;
    b
}

fn truncate(mut b: Vec<u8>, n: u16) -> Vec<u8> {
    b.truncate(n as usize);
    b
}

struct ControlState {
    configuration: u8,
    /// Alternate setting of the streaming interface.
    streaming: bool,
    mute: bool,
    /// In 1/256 dB; `VOLUME_SILENT` is minus infinity.
    volume: i16,
}

/// Pulls the phone's audio for the capture endpoint.
struct Capture {
    playout: Playout,
    mono: Vec<f32>,
}

pub struct Device {
    shared: Arc<Shared>,
    /// Whether Celmic routes the phone into this microphone (it is the
    /// selected output). Otherwise the microphone records silence.
    pub feeding: AtomicBool,
    descriptors: HashMap<(u8, u8), Vec<u8>>,
    state: Mutex<ControlState>,
    capture: Mutex<Capture>,
}

impl Device {
    pub fn new(shared: Arc<Shared>) -> Device {
        let mut descriptors = HashMap::new();
        descriptors.insert((DESCRIPTOR_DEVICE, 0), device_descriptor());
        descriptors.insert((DESCRIPTOR_CONFIGURATION, 0), configuration_descriptor());
        descriptors.insert((DESCRIPTOR_STRING, 0), vec![4, DESCRIPTOR_STRING, 0x09, 0x04]); // en-US
        descriptors.insert((DESCRIPTOR_STRING, 1), string_descriptor("Celmic"));
        descriptors.insert((DESCRIPTOR_STRING, 2), string_descriptor(PRODUCT));
        descriptors.insert((DESCRIPTOR_STRING, 3), string_descriptor("CELMIC-1"));
        Device {
            shared,
            feeding: AtomicBool::new(false),
            descriptors,
            state: Mutex::new(ControlState { configuration: 0, streaming: false, mute: false, volume: 0 }),
            capture: Mutex::new(Capture { playout: Playout::new(SAMPLE_RATE), mono: Vec::new() }),
        }
    }

    /// An app is recording from the microphone.
    pub fn streaming(&self) -> bool {
        let s = self.state.lock().unwrap();
        s.configuration == 1 && s.streaming
    }

    /// Handles the standard requests and the UAC2 class requests
    /// usbaudio2.sys makes. Returns the IN data and a status.
    pub fn handle_control(&self, setup: SetupPacket, out: &[u8]) -> (Vec<u8>, i32) {
        let mut s = self.state.lock().unwrap();
        let reply = |value: Vec<u8>| (truncate(value, setup.length), 0);

        if setup.request_type & 0x60 == 0x00 {
            return match setup.request {
                REQUEST_GET_DESCRIPTOR => {
                    match self.descriptors.get(&((setup.value >> 8) as u8, setup.value as u8)) {
                        Some(d) => reply(d.clone()),
                        None => (Vec::new(), STALL), // e.g. device qualifier: full speed only
                    }
                }
                REQUEST_SET_ADDRESS | REQUEST_CLEAR_FEATURE | REQUEST_SET_FEATURE => (Vec::new(), 0),
                REQUEST_SET_CONFIGURATION if setup.value <= 1 => {
                    s.configuration = setup.value as u8;
                    s.streaming = false;
                    (Vec::new(), 0)
                }
                REQUEST_GET_CONFIGURATION => reply(vec![s.configuration]),
                REQUEST_SET_INTERFACE => match (setup.index, setup.value) {
                    (0, 0) if s.configuration == 1 => (Vec::new(), 0),
                    (1, alt @ (0 | 1)) if s.configuration == 1 => {
                        s.streaming = alt == 1;
                        (Vec::new(), 0)
                    }
                    _ => (Vec::new(), STALL),
                },
                REQUEST_GET_INTERFACE => match setup.index {
                    0 => reply(vec![0]),
                    1 => reply(vec![s.streaming as u8]),
                    _ => (Vec::new(), STALL),
                },
                REQUEST_GET_STATUS => reply(vec![0, 0]),
                // A stable frame number; stalling would fail the host's query.
                REQUEST_SYNCH_FRAME => reply(vec![0, 0]),
                _ => (Vec::new(), STALL),
            };
        }

        // Class requests to an entity of the AudioControl interface: wIndex
        // holds the entity ID and interface, wValue the control and channel.
        let get = setup.request_type & 0x80 != 0;
        let entity = (setup.index >> 8) as u8;
        let control = (setup.value >> 8) as u8;
        if setup.request_type & 0x1f != 0x01 || setup.index as u8 != 0 || setup.value as u8 != 0 {
            return (Vec::new(), STALL);
        }

        match (entity, control, setup.request, get) {
            (CLOCK_SOURCE, CS_SAM_FREQ_CONTROL, AUDIO_CUR, true) => reply(SAMPLE_RATE.to_le_bytes().to_vec()),
            (CLOCK_SOURCE, CS_SAM_FREQ_CONTROL, AUDIO_RANGE, true) => {
                // One discrete subrange: MIN = MAX = 48000, RES = 0.
                let mut range = 1u16.to_le_bytes().to_vec();
                for value in [SAMPLE_RATE, SAMPLE_RATE, 0] {
                    range.extend_from_slice(&value.to_le_bytes());
                }
                reply(range)
            }
            // Read-only, but setting the frequency it already has is harmless.
            (CLOCK_SOURCE, CS_SAM_FREQ_CONTROL, AUDIO_CUR, false) if out.get(..4) == Some(&SAMPLE_RATE.to_le_bytes()) => {
                (Vec::new(), 0)
            }
            (CLOCK_SOURCE, CS_CLOCK_VALID_CONTROL, AUDIO_CUR, true) => reply(vec![1]),

            (FEATURE_UNIT, FU_MUTE_CONTROL, AUDIO_CUR, true) => reply(vec![s.mute as u8]),
            (FEATURE_UNIT, FU_MUTE_CONTROL, AUDIO_CUR, false) => {
                if let Some(&first) = out.first() {
                    s.mute = first != 0;
                }
                (Vec::new(), 0)
            }
            (FEATURE_UNIT, FU_VOLUME_CONTROL, AUDIO_CUR, true) => reply(s.volume.to_le_bytes().to_vec()),
            (FEATURE_UNIT, FU_VOLUME_CONTROL, AUDIO_CUR, false) => {
                if let Some(v) = out.get(..2) {
                    let volume = i16::from_le_bytes([v[0], v[1]]);
                    s.volume = if volume == VOLUME_SILENT { volume } else { volume.clamp(VOLUME_MIN, VOLUME_MAX) };
                }
                (Vec::new(), 0)
            }
            (FEATURE_UNIT, FU_VOLUME_CONTROL, AUDIO_RANGE, true) => {
                let mut range = 1u16.to_le_bytes().to_vec();
                for value in [VOLUME_MIN, VOLUME_MAX, VOLUME_RES] {
                    range.extend_from_slice(&value.to_le_bytes());
                }
                reply(range)
            }
            _ => (Vec::new(), STALL),
        }
    }

    /// The linear gain Windows set through the feature unit; 0 when muted.
    fn gain(&self) -> f32 {
        let s = self.state.lock().unwrap();
        if s.mute || s.volume == VOLUME_SILENT {
            0.0
        } else {
            10f32.powf(s.volume as f32 / 256.0 / 20.0)
        }
    }

    /// Fills a capture URB: packets laid out back to back in `out` with the
    /// given lengths, one 1 ms packet per bus frame.
    pub fn read_capture(&self, out: &mut [u8], lengths: impl IntoIterator<Item = usize>) {
        if !self.streaming() || !self.feeding.load(Ordering::Relaxed) {
            out.fill(0);
            return;
        }
        let gain = self.gain();
        let target = self.shared.target_latency_ms.load(Ordering::Relaxed);
        let mut capture = self.capture.lock().unwrap();
        let Capture { playout, mono } = &mut *capture;
        let mut pos = 0;
        for length in lengths {
            let length = length.min(out.len() - pos);
            let packet = &mut out[pos..pos + length];
            pos += length;
            let frames = length / FRAME_BYTES;
            mono.resize(frames, 0.0);
            playout.fill(&self.shared.jitter, target, mono);
            for (frame, &sample) in packet.chunks_exact_mut(FRAME_BYTES).zip(mono.iter()) {
                let value = ((sample * gain).clamp(-1.0, 1.0) * 8_388_607.0) as i32;
                let bytes = &value.to_le_bytes()[..SUBSLOT_BYTES];
                frame[..SUBSLOT_BYTES].copy_from_slice(bytes);
                frame[SUBSLOT_BYTES..].copy_from_slice(bytes);
            }
            packet[frames * FRAME_BYTES..].fill(0);
        }
        out[pos..].fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le16(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[0], b[1]])
    }

    /// Every descriptor of the configuration, in order.
    fn parts(config: &[u8]) -> Vec<&[u8]> {
        let mut found = Vec::new();
        let mut pos = 0;
        while pos < config.len() {
            let length = config[pos] as usize;
            assert!(length >= 2 && pos + length <= config.len(), "descriptor at {pos}");
            found.push(&config[pos..pos + length]);
            pos += length;
        }
        found
    }

    fn device() -> Device {
        Device::new(Arc::new(Shared::new()))
    }

    fn control(d: &Device, request_type: u8, request: u8, value: u16, index: u16, length: u16, out: &[u8]) -> (Vec<u8>, i32) {
        d.handle_control(SetupPacket { request_type, request, value, index, length }, out)
    }

    #[test]
    fn device_descriptor_is_uac2_composite() {
        let d = device_descriptor();
        assert_eq!((d.len(), d[0], d[1]), (18, 18, DESCRIPTOR_DEVICE));
        assert_eq!(le16(&d[2..4]), 0x0200);
        assert_eq!(&d[4..7], &[0xEF, 0x02, 0x01], "interface association class");
    }

    #[test]
    fn configuration_lengths_and_interfaces() {
        let config = configuration_descriptor();
        assert_eq!(le16(&config[2..4]) as usize, config.len());
        let all = parts(&config);
        assert_eq!(all[1][1], DESCRIPTOR_INTERFACE_ASSOCIATION);
        assert_eq!(&all[1][2..7], &[0, INTERFACES, 0x01, 0x00, IP_VERSION_02_00]);
        let interfaces: Vec<_> = all.iter().filter(|b| b[1] == DESCRIPTOR_INTERFACE).collect();
        assert_eq!(interfaces.len(), 3, "AC, and two alternate settings of AS");
        for i in interfaces {
            assert_eq!((i[5], i[7]), (0x01, IP_VERSION_02_00), "audio class, UAC2 protocol");
        }
        // The AC header's total covers every class-specific AC descriptor.
        let start = all.iter().position(|b| b[1] == CS_INTERFACE && b[2] == 0x01).unwrap();
        let ac: usize = all[start..].iter().take_while(|b| b[1] == CS_INTERFACE).map(|b| b.len()).sum();
        assert_eq!(le16(&all[start][6..8]) as usize, ac);
    }

    #[test]
    fn capture_endpoint() {
        let config = configuration_descriptor();
        let endpoints: Vec<_> = parts(&config).into_iter().filter(|b| b[1] == DESCRIPTOR_ENDPOINT).collect();
        assert_eq!(endpoints.len(), 1);
        let ep = endpoints[0];
        assert_eq!((ep[2], ep[3]), (0x81, 0x0D), "IN 1, isochronous synchronous");
        assert_eq!(le16(&ep[4..6]) as usize, PACKET_BYTES);
    }

    #[test]
    fn string_descriptors() {
        let d = device();
        let (data, status) = control(&d, 0x80, REQUEST_GET_DESCRIPTOR, 0x0302, 0x0409, 255, &[]);
        assert_eq!(status, 0);
        let name: Vec<u16> = data[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(String::from_utf16(&name).unwrap(), PRODUCT);
        assert_eq!(control(&d, 0x80, REQUEST_GET_DESCRIPTOR, 0x0600, 0, 10, &[]).1, STALL, "no device qualifier");
    }

    #[test]
    fn clock_requests() {
        let d = device();
        let clock = (CLOCK_SOURCE as u16) << 8;
        let freq = (CS_SAM_FREQ_CONTROL as u16) << 8;
        assert_eq!(control(&d, 0xA1, AUDIO_CUR, freq, clock, 4, &[]), (48_000u32.to_le_bytes().to_vec(), 0));
        assert_eq!(control(&d, 0xA1, AUDIO_RANGE, freq, clock, 2, &[]), (vec![1, 0], 0));
        assert_eq!(control(&d, 0x21, AUDIO_CUR, freq, clock, 4, &44_100u32.to_le_bytes()).1, STALL);
    }

    #[test]
    fn streams_only_with_alternate_setting_1() {
        let d = device();
        assert!(!d.streaming());
        control(&d, 0x00, REQUEST_SET_CONFIGURATION, 1, 0, 0, &[]);
        control(&d, 0x01, REQUEST_SET_INTERFACE, 1, 1, 0, &[]);
        assert!(d.streaming());
        assert_eq!(control(&d, 0x81, REQUEST_GET_INTERFACE, 0, 1, 1, &[]).0, vec![1]);
        control(&d, 0x01, REQUEST_SET_INTERFACE, 0, 1, 0, &[]);
        assert!(!d.streaming());
        assert_eq!(control(&d, 0x01, REQUEST_SET_INTERFACE, 2, 1, 0, &[]).1, STALL);
    }

    #[test]
    fn capture_plays_the_phone_with_feature_unit_gain() {
        let d = device();
        control(&d, 0x00, REQUEST_SET_CONFIGURATION, 1, 0, 0, &[]);
        control(&d, 0x01, REQUEST_SET_INTERFACE, 1, 1, 0, &[]);
        d.shared.jitter.push(&vec![0.5; 9600], SAMPLE_RATE); // 200 ms
        let sample = |d: &Device| {
            let mut out = vec![0xEE; PACKET_BYTES * 2];
            d.read_capture(&mut out, [PACKET_BYTES, PACKET_BYTES]);
            let at = PACKET_BYTES + FRAME_BYTES * 10;
            let left = i32::from_le_bytes([out[at], out[at + 1], out[at + 2], 0]);
            let right = i32::from_le_bytes([out[at + 3], out[at + 4], out[at + 5], 0]);
            assert_eq!(left, right, "mono on both channels");
            left as f32 / 8_388_607.0
        };

        assert_eq!(sample(&d), 0.0, "silent until Celmic routes the phone here");
        d.feeding.store(true, Ordering::Relaxed);
        assert!((sample(&d) - 0.5).abs() < 1e-4);

        let volume = (FU_VOLUME_CONTROL as u16) << 8;
        let unit = (FEATURE_UNIT as u16) << 8;
        control(&d, 0x21, AUDIO_CUR, volume, unit, 2, &(-6i16 * 256).to_le_bytes());
        assert!((sample(&d) - 0.25).abs() < 0.01, "-6 dB");
        control(&d, 0x21, AUDIO_CUR, (FU_MUTE_CONTROL as u16) << 8, unit, 1, &[1]);
        assert_eq!(sample(&d), 0.0, "muted");
    }
}
