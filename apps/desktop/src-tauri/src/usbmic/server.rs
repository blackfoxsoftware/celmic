//! The USB/IP server: exports the microphone on localhost and services its
//! URBs on the host's bus-frame schedule.

use std::collections::{HashMap, VecDeque};
use std::io::{self, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL};

use super::clock::{self, TimerResolution};
use super::protocol::*;
use super::uac2::{self, CAPTURE_ENDPOINT, Device, SetupPacket};
use super::{BUS_ID, TCP_PORT};
use crate::server::Shared;

/// The microphone and its USB/IP server.
pub struct UsbMic {
    device: Arc<Device>,
    sessions: Arc<Sessions>,
}

/// The attached session, if any. A new import replaces it: usbip-win2 may
/// reattach before it notices that the old connection is gone.
#[derive(Default)]
struct Sessions {
    current: Mutex<Option<(u64, TcpStream)>>,
    next_id: AtomicU64,
}

impl UsbMic {
    /// Listens on localhost; usbip-win2 attaches from there.
    pub fn start(shared: Arc<Shared>) -> io::Result<UsbMic> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, TCP_PORT))?;
        let device = Arc::new(Device::new(shared));
        let sessions = Arc::new(Sessions::default());
        let (thread_device, thread_sessions) = (device.clone(), sessions.clone());
        thread::Builder::new().name("celmic-usbip".into()).spawn(move || {
            for conn in listener.incoming().flatten() {
                let (device, sessions) = (thread_device.clone(), thread_sessions.clone());
                let _ = thread::Builder::new()
                    .name("celmic-usbip-conn".into())
                    .spawn(move || handle_connection(conn, &device, &sessions));
            }
        })?;
        Ok(UsbMic { device, sessions })
    }

    /// Windows has the microphone attached.
    pub fn attached(&self) -> bool {
        self.sessions.current.lock().unwrap().is_some()
    }

    /// An app is recording from the microphone.
    pub fn streaming(&self) -> bool {
        self.attached() && self.device.streaming()
    }

    /// Routes the phone into the microphone (or silence).
    pub fn set_feeding(&self, on: bool) {
        self.device.feeding.store(on, Ordering::Relaxed);
    }
}

fn handle_connection(conn: TcpStream, device: &Arc<Device>, sessions: &Sessions) {
    let _ = conn.set_nodelay(true);
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let Ok(mut writer) = conn.try_clone() else { return };
    let mut reader = BufReader::with_capacity(64 * 1024, &conn);

    match read_op_header(&mut reader) {
        Ok(OP_REQ_DEVLIST) => {
            let mut reply = Vec::new();
            append_op_header(&mut reply, OP_REP_DEVLIST, 0);
            reply.extend_from_slice(&1u32.to_be_bytes());
            append_device_record(&mut reply);
            // One class/subclass/protocol tuple per interface.
            reply.extend_from_slice(&[0x01, 0x01, 0x20, 0]); // AudioControl, UAC2
            reply.extend_from_slice(&[0x01, 0x02, 0x20, 0]); // AudioStreaming, UAC2
            let _ = writer.write_all(&reply);
        }
        Ok(OP_REQ_IMPORT) => {
            let mut bus_id = [0u8; 32];
            if reader.read_exact(&mut bus_id).is_err() {
                return;
            }
            let mut reply = Vec::new();
            if String::from_utf8_lossy(&bus_id).trim_end_matches('\0') != BUS_ID {
                append_op_header(&mut reply, OP_REP_IMPORT, 1);
                let _ = writer.write_all(&reply);
                return;
            }
            let id = sessions.next_id.fetch_add(1, Ordering::Relaxed);
            {
                let mut current = sessions.current.lock().unwrap();
                if let Some((_, old)) = current.take() {
                    let _ = old.shutdown(Shutdown::Both);
                }
                *current = conn.try_clone().ok().map(|stream| (id, stream));
            }
            append_op_header(&mut reply, OP_REP_IMPORT, 0);
            append_device_record(&mut reply);
            if writer.write_all(&reply).is_ok() {
                // An idle microphone gets no URBs; the session must not time out.
                let _ = conn.set_read_timeout(None);
                Session::run(reader, writer, device.clone());
            }
            let mut current = sessions.current.lock().unwrap();
            if current.as_ref().is_some_and(|(current_id, _)| *current_id == id) {
                *current = None;
            }
        }
        _ => {}
    }
}

/// Appends the 312-byte USB/IP device record.
fn append_device_record(b: &mut Vec<u8>) {
    let mut path = [0u8; 256];
    let mut bus_id = [0u8; 32];
    fixed_string(&mut path, &format!("/sys/devices/platform/celmic/{BUS_ID}"));
    fixed_string(&mut bus_id, BUS_ID);
    b.extend_from_slice(&path);
    b.extend_from_slice(&bus_id);
    let dev = uac2::device_descriptor();
    let le16 = |at: usize| u16::from_le_bytes([dev[at], dev[at + 1]]);
    b.extend_from_slice(&1u32.to_be_bytes()); // bus number
    b.extend_from_slice(&1u32.to_be_bytes()); // device number
    b.extend_from_slice(&SPEED_FULL.to_be_bytes());
    for at in [8, 10, 12] {
        b.extend_from_slice(&le16(at).to_be_bytes()); // vendor, product, bcdDevice
    }
    b.extend_from_slice(&dev[4..7]); // class, subclass, protocol
    b.extend_from_slice(&[1, dev[17], uac2::INTERFACES]); // configuration value, configurations, interfaces
}

/// One isochronous capture URB, waiting for its last frame to elapse.
struct Job {
    submit: Submit,
    start: u64,
    end: u64,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Schedule {
    /// Isochronous URBs that still need exactly one reply, by sequence.
    pending: HashMap<u32, Arc<AtomicBool>>,
    /// The frame after the last scheduled URB.
    next_frame: Option<u64>,
}

impl Schedule {
    /// Places a transfer as a host controller does for an ASAP isochronous
    /// URB: right after the endpoint's previous URB, or on the next frame if
    /// that slot has already begun (Windows queues several URBs, so the
    /// schedule, not the arrival time, sets the cadence).
    fn reserve(&mut self, packets: u64, current: u64) -> (u64, u64) {
        let start = match self.next_frame {
            Some(next) if next > current => next,
            _ => current + 1,
        };
        let end = start + packets;
        self.next_frame = Some(end);
        (start, end)
    }

    /// Starts the next URB on a fresh schedule once no URB is in flight: the
    /// host stopped, aborted or reset the stream.
    fn reset_if_idle(&mut self) {
        if self.pending.is_empty() {
            self.next_frame = None;
        }
    }

    /// Takes a pending URB so that exactly one reply is sent for it.
    fn claim(&mut self, sequence: u32, unlinked: bool) -> bool {
        let Some(cancel) = self.pending.remove(&sequence) else {
            return false;
        };
        if unlinked {
            cancel.store(true, Ordering::Relaxed);
            self.reset_if_idle();
        }
        true
    }
}

struct Session {
    device: Arc<Device>,
    writer: Mutex<(TcpStream, Vec<u8>)>,
    schedule: Mutex<Schedule>,
    queue: Mutex<VecDeque<Job>>,
    wake: Condvar,
    stopping: AtomicBool,
}

fn raise_thread_priority() {
    // SAFETY: the pseudo handle of the calling thread.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) };
}

impl Session {
    fn run(mut reader: BufReader<&TcpStream>, writer: TcpStream, device: Arc<Device>) {
        let _resolution = TimerResolution::acquire();
        raise_thread_priority();
        let session = Arc::new(Session {
            device,
            writer: Mutex::new((writer, Vec::new())),
            schedule: Mutex::default(),
            queue: Mutex::default(),
            wake: Condvar::new(),
            stopping: AtomicBool::new(false),
        });
        let worker = {
            let session = session.clone();
            thread::Builder::new().name("celmic-usbip-iso".into()).spawn(move || session.complete_urbs())
        };

        let ended = loop {
            if let Err(e) = session.handle_command(&mut reader) {
                break e;
            }
        };
        eprintln!("Celmic Microphone: USB/IP session ended: {ended}");

        session.stopping.store(true, Ordering::Relaxed);
        session.wake.notify_all();
        if let Ok(worker) = worker {
            let _ = worker.join();
        }
    }

    fn handle_command(&self, reader: &mut impl Read) -> io::Result<()> {
        let basic = read_basic_header(reader)?;
        match basic.command {
            CMD_SUBMIT => {
                let submit = read_submit(reader, basic)?;
                if basic.endpoint == 0 {
                    return self.control(&submit);
                }
                if !(submit.is_isochronous() && basic.endpoint == CAPTURE_ENDPOINT && basic.direction == DIRECTION_IN) {
                    return self.reply(&submit, None, STATUS_PIPE, 0, &[], &[]);
                }
                let cancel = Arc::new(AtomicBool::new(false));
                let (start, end) = {
                    let mut schedule = self.schedule.lock().unwrap();
                    schedule.pending.insert(basic.sequence, cancel.clone());
                    schedule.reserve(submit.packets.len() as u64, clock::frame())
                };
                self.queue.lock().unwrap().push_back(Job { submit, start, end, cancel });
                self.wake.notify_all();
                Ok(())
            }
            CMD_UNLINK => {
                let target = read_unlink(reader)?;
                let mut writer = self.writer.lock().unwrap();
                let found = self.schedule.lock().unwrap().claim(target, true);
                self.wake.notify_all();
                let (stream, frame) = &mut *writer;
                frame.clear();
                // The protocol answers a successful unlink with -ECONNRESET.
                append_ret_unlink(frame, basic.sequence, if found { STATUS_CONN_RESET } else { STATUS_OK });
                stream.write_all(frame)
            }
            command => Err(io::Error::new(io::ErrorKind::InvalidData, format!("USB/IP command {command}"))),
        }
    }

    /// Endpoint zero is serialized: enumeration depends on request order.
    fn control(&self, submit: &Submit) -> io::Result<()> {
        let setup = SetupPacket::parse(&submit.setup);
        let (mut data, status) = self.device.handle_control(setup, &submit.out);
        if status == STATUS_OK && (setup.is_endpoint_halt_clear() || setup.restarts_streams()) {
            self.schedule.lock().unwrap().reset_if_idle();
        }
        data.truncate(submit.transfer_buffer_length as usize);
        self.reply(submit, None, if status == 0 { STATUS_OK } else { STATUS_PIPE }, 0, &data, &[])
    }

    /// Sends RET_SUBMIT. A `tracked` (isochronous) URB is claimed first: if an
    /// UNLINK claimed it already, RET_UNLINK was its only reply.
    fn reply(
        &self,
        submit: &Submit,
        tracked: Option<u32>,
        status: i32,
        start_frame: u32,
        data: &[u8],
        packets: &[IsoPacket],
    ) -> io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        if let Some(sequence) = tracked {
            if !self.schedule.lock().unwrap().claim(sequence, false) {
                return Ok(());
            }
        }
        let (stream, frame) = &mut *writer;
        frame.clear();
        append_ret_submit(frame, submit, status, start_frame, data, packets);
        stream.write_all(frame)
    }

    /// Completes capture URBs in order, each once its last frame has passed
    /// on the coarse bus clock, filled with the audio of that moment.
    fn complete_urbs(&self) {
        raise_thread_priority();
        let mut audio = Vec::new();
        loop {
            let mut job = {
                let mut queue = self.queue.lock().unwrap();
                loop {
                    if self.stopping.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Some(job) = queue.pop_front() {
                        break job;
                    }
                    queue = self.wake.wait(queue).unwrap();
                }
            };

            let cancelled = || job.cancel.load(Ordering::Relaxed) || self.stopping.load(Ordering::Relaxed);
            while !cancelled() {
                let left = clock::until_frame(job.end);
                if left > Duration::from_millis(2) {
                    // Woken early by unlinks and new work; the loop re-checks.
                    let queue = self.queue.lock().unwrap();
                    let _ = self.wake.wait_timeout(queue, left - Duration::from_millis(1)).unwrap();
                } else if clock::frame() < job.end {
                    thread::sleep(Duration::from_micros(200));
                } else {
                    break;
                }
            }
            if cancelled() {
                continue;
            }

            let total = job.submit.packets.iter().map(|p| p.length as usize).sum::<usize>();
            audio.resize(total.min(job.submit.transfer_buffer_length as usize), 0);
            self.device.read_capture(&mut audio, job.submit.packets.iter().map(|p| p.length as usize));
            let mut remaining = audio.len();
            for packet in &mut job.submit.packets {
                packet.actual_length = (packet.length as usize).min(remaining) as u32;
                packet.status = STATUS_OK;
                remaining -= packet.actual_length as usize;
            }
            let (submit, sequence) = (&job.submit, job.submit.basic.sequence);
            // usbaudio2.sys tracks the stream position from the frame we report.
            if self.reply(submit, Some(sequence), STATUS_OK, job.start as u32, &audio, &submit.packets).is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_follows_on_and_restarts_when_late() {
        let mut s = Schedule::default();
        assert_eq!(s.reserve(10, 100), (101, 111), "a new stream starts on the next frame");
        assert_eq!(s.reserve(10, 102), (111, 121), "queued URBs follow on");
        assert_eq!(s.reserve(10, 130), (131, 141), "a late URB moves to the next frame");
    }

    #[test]
    fn unlink_claims_once_and_resets_an_idle_stream() {
        let mut s = Schedule::default();
        let cancel = Arc::new(AtomicBool::new(false));
        s.pending.insert(7, cancel.clone());
        s.reserve(10, 100);
        assert!(s.claim(7, true));
        assert!(cancel.load(Ordering::Relaxed));
        assert!(!s.claim(7, false), "the completion must not reply again");
        assert_eq!(s.next_frame, None);
    }

    #[test]
    fn device_record_is_312_bytes() {
        let mut b = Vec::new();
        append_device_record(&mut b);
        assert_eq!(b.len(), 312);
        assert_eq!(&b[256..259], BUS_ID.as_bytes());
        assert_eq!(b[311], uac2::INTERFACES);
    }
}
