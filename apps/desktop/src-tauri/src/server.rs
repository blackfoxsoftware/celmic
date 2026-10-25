//! UDP receiver: pairing handshake, session tracking and PCM ingestion.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::jitter::JitterBuffer;
use crate::protocol::{self, Header, PacketType, RejectReason};

const SESSION_TIMEOUT: Duration = Duration::from_secs(5);
/// Minimum spacing between REJECTs sent to the same address.
const REJECT_INTERVAL: Duration = Duration::from_millis(500);
/// After a wrong code, HELLOs from that IP are ignored for this long.
const BAD_CODE_COOLDOWN: Duration = Duration::from_secs(1);
/// Wrong codes tolerated before the pairing code is rotated. With 6 digits this
/// leaves a 1e-5 chance per code; an attacker can only cause re-pairing.
const MAX_BAD_CODES: u32 = 10;
/// How long a kicked/replaced session keeps being told to stop.
const EVICTION_MEMORY: Duration = Duration::from_secs(60);

/// State shared between the network thread, the audio callback and the UI.
pub struct Shared {
    pub jitter: JitterBuffer,
    gain_bits: AtomicU32,
    pub target_latency_ms: AtomicU32,
    /// Peak level since the last status tick, as f32 bits.
    peak_bits: AtomicU32,
    pub packets: AtomicU64,
    pub lost: AtomicU64,
    code: Mutex<String>,
    bad_codes: AtomicU32,
    /// The current code was generated because of too many wrong attempts.
    code_rotated_for_safety: AtomicBool,
    client: Mutex<Option<Client>>,
    evicted: Mutex<Vec<Evicted>>,
}

#[derive(Clone)]
struct Client {
    /// Latest source address. Identity is (IP, session): iOS may switch ports.
    addr: SocketAddr,
    name: String,
    session: u32,
    sample_rate: u32,
    last_seen: Instant,
    connected_at: Instant,
    next_seq: Option<u32>,
}

impl Client {
    fn matches(&self, session: u32, from: SocketAddr) -> bool {
        self.session == session && self.addr.ip() == from.ip()
    }
}

struct Evicted {
    ip: IpAddr,
    session: u32,
    reason: RejectReason,
    at: Instant,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub address: String,
    pub sample_rate: u32,
    pub connected_secs: u64,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            jitter: JitterBuffer::new(),
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            target_latency_ms: AtomicU32::new(40),
            peak_bits: AtomicU32::new(0),
            packets: AtomicU64::new(0),
            lost: AtomicU64::new(0),
            code: Mutex::new(new_code()),
            bad_codes: AtomicU32::new(0),
            code_rotated_for_safety: AtomicBool::new(false),
            client: Mutex::new(None),
            evicted: Mutex::new(Vec::new()),
        }
    }

    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain_bits.store(gain.clamp(0.0, 8.0).to_bits(), Ordering::Relaxed);
    }

    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak_bits.swap(0, Ordering::Relaxed))
    }

    fn record_peak(&self, peak: f32) {
        // Racy max is fine: only the network thread writes, the ticker resets.
        if peak > f32::from_bits(self.peak_bits.load(Ordering::Relaxed)) {
            self.peak_bits.store(peak.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn code(&self) -> String {
        self.code.lock().unwrap().clone()
    }

    pub fn code_rotated_for_safety(&self) -> bool {
        self.code_rotated_for_safety.load(Ordering::Relaxed)
    }

    pub fn regenerate_code(&self) -> String {
        self.code_rotated_for_safety.store(false, Ordering::Relaxed);
        self.rotate_code()
    }

    fn rotate_code(&self) -> String {
        let code = new_code();
        *self.code.lock().unwrap() = code.clone();
        self.bad_codes.store(0, Ordering::Relaxed);
        code
    }

    /// Counts a wrong pairing code; rotates the code once there are too many.
    fn record_bad_code(&self) {
        if self.bad_codes.fetch_add(1, Ordering::Relaxed) + 1 >= MAX_BAD_CODES {
            self.rotate_code();
            self.code_rotated_for_safety.store(true, Ordering::Relaxed);
        }
    }

    pub fn client_info(&self) -> Option<ClientInfo> {
        self.client.lock().unwrap().as_ref().map(|c| ClientInfo {
            name: c.name.clone(),
            address: c.addr.to_string(),
            sample_rate: c.sample_rate,
            connected_secs: c.connected_at.elapsed().as_secs(),
        })
    }

    /// The user kicked the phone: it is told to stop instead of reconnecting.
    pub fn disconnect(&self) {
        self.end_session(Some(RejectReason::Kicked));
    }

    fn end_session(&self, evict: Option<RejectReason>) {
        let previous = self.client.lock().unwrap().take();
        if let (Some(client), Some(reason)) = (previous, evict) {
            self.evict(&client, reason);
        }
        self.jitter.clear();
    }

    fn evict(&self, client: &Client, reason: RejectReason) {
        let mut evicted = self.evicted.lock().unwrap();
        evicted.retain(|e| e.at.elapsed() < EVICTION_MEMORY);
        evicted.push(Evicted { ip: client.addr.ip(), session: client.session, reason, at: Instant::now() });
    }

    fn eviction_reason(&self, session: u32, from: SocketAddr) -> Option<RejectReason> {
        self.evicted
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.session == session && e.ip == from.ip() && e.at.elapsed() < EVICTION_MEMORY)
            .map(|e| e.reason)
    }

    /// An explicit new HELLO from a previously evicted phone (user pressed retry).
    fn forget_evictions(&self, ip: IpAddr) {
        self.evicted.lock().unwrap().retain(|e| e.ip != ip);
    }
}

fn new_code() -> String {
    format!("{:06}", fastrand::u32(0..1_000_000))
}

/// Drops entries older than `max_age` once a map grows, to keep it bounded.
fn prune<K>(map: &mut HashMap<K, Instant>, max_age: Duration) {
    if map.len() > 256 {
        map.retain(|_, t| t.elapsed() < max_age);
    }
}

/// Binds the UDP socket and spawns the receive loop.
pub fn spawn(shared: Arc<Shared>, port: u16, desktop_name: String) -> io::Result<()> {
    let socket = UdpSocket::bind(("0.0.0.0", port))?;
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;

    thread::Builder::new()
        .name("celmic-net".into())
        .spawn(move || Server::new(socket, shared, desktop_name).run())?;
    Ok(())
}

struct Server {
    socket: UdpSocket,
    shared: Arc<Shared>,
    desktop_name: String,
    last_reject: HashMap<SocketAddr, Instant>,
    last_bad_code: HashMap<IpAddr, Instant>,
    pcm: Vec<f32>,
}

impl Server {
    fn new(socket: UdpSocket, shared: Arc<Shared>, desktop_name: String) -> Self {
        Self {
            socket,
            shared,
            desktop_name,
            last_reject: HashMap::new(),
            last_bad_code: HashMap::new(),
            pcm: Vec::with_capacity(4096),
        }
    }

    fn run(mut self) {
        let mut buf = [0u8; 2048];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, from)) => self.handle(&buf[..len], from),
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
                // Windows reports ICMP "port unreachable" from earlier sends as ConnectionReset.
                Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {}
                Err(e) => {
                    eprintln!("celmic: udp recv error: {e}");
                    thread::sleep(Duration::from_millis(100));
                }
            }
            self.expire_session();
        }
    }

    fn expire_session(&self) {
        let expired = matches!(
            self.shared.client.lock().unwrap().as_ref(),
            Some(c) if c.last_seen.elapsed() > SESSION_TIMEOUT
        );
        if expired {
            // Not evicted: if the phone comes back (Wi-Fi blip) it may re-pair.
            self.shared.end_session(None);
        }
    }

    fn handle(&mut self, datagram: &[u8], from: SocketAddr) {
        let Ok((header, payload)) = protocol::parse(datagram) else {
            return;
        };
        if header.version != protocol::VERSION {
            self.reject(from, header.session, RejectReason::UnsupportedVersion);
            return;
        }
        match header.kind {
            PacketType::Hello => self.on_hello(payload, from),
            PacketType::Audio => self.on_audio(&header, payload, from),
            PacketType::Ping => {
                if self.touch(header.session, from) {
                    self.send(from, &Header::new(PacketType::Pong, header.session), payload);
                }
            }
            PacketType::Bye => {
                if self.touch(header.session, from) {
                    self.shared.end_session(None);
                }
            }
            // Server-to-client packets; ignore if echoed back at us.
            PacketType::Welcome | PacketType::Reject | PacketType::Pong => {}
        }
    }

    fn on_hello(&mut self, payload: &[u8], from: SocketAddr) {
        let Some((code, name)) = protocol::parse_hello(payload) else {
            return;
        };
        // Throttle guessing: one wrong code per IP per second.
        if self.last_bad_code.get(&from.ip()).is_some_and(|t| t.elapsed() < BAD_CODE_COOLDOWN) {
            return;
        }
        if code != self.shared.code() {
            prune(&mut self.last_bad_code, BAD_CODE_COOLDOWN);
            self.last_bad_code.insert(from.ip(), Instant::now());
            self.shared.record_bad_code();
            self.reject(from, 0, RejectReason::BadCode);
            return;
        }
        self.shared.bad_codes.store(0, Ordering::Relaxed);
        self.shared.forget_evictions(from.ip());

        let (session, replaced) = {
            let mut client = self.shared.client.lock().unwrap();
            match client.as_mut() {
                // Retransmitted HELLO (possibly from another source port, as on
                // iOS where each candidate address has its own socket): same session.
                Some(c) if c.addr.ip() == from.ip() => {
                    c.addr = from;
                    c.last_seen = Instant::now();
                    (c.session, None)
                }
                _ => {
                    let session = fastrand::u32(1..);
                    let previous = client.replace(Client {
                        addr: from,
                        name: if name.is_empty() { "Phone".into() } else { name },
                        session,
                        sample_rate: 0,
                        last_seen: Instant::now(),
                        connected_at: Instant::now(),
                        next_seq: None,
                    });
                    (session, Some(previous))
                }
            }
        };
        if let Some(previous) = replaced {
            // Last phone wins; the previous one is told to stop, not to retry.
            if let Some(previous) = previous {
                self.shared.evict(&previous, RejectReason::Replaced);
            }
            self.shared.jitter.clear();
        }
        self.send(from, &Header::new(PacketType::Welcome, session), self.desktop_name.as_bytes());
    }

    fn on_audio(&mut self, header: &Header, payload: &[u8], from: SocketAddr) {
        if header.codec != protocol::CODEC_PCM_S16LE
            || header.channels != 1
            || !protocol::SAMPLE_RATES.contains(&header.sample_rate)
        {
            return;
        }

        enum Verdict {
            Play,
            Drop,
            Reject,
        }
        let verdict = {
            let mut guard = self.shared.client.lock().unwrap();
            match guard.as_mut() {
                Some(client) if client.matches(header.session, from) => {
                    client.addr = from;
                    client.last_seen = Instant::now();
                    client.sample_rate = header.sample_rate;
                    let delta = client
                        .next_seq
                        .map_or(0, |expected| header.seq.wrapping_sub(expected) as i32);
                    if delta < 0 {
                        // Late or duplicate packet: playout has already moved past it.
                        Verdict::Drop
                    } else {
                        if delta > 0 {
                            self.shared.lost.fetch_add(delta as u64, Ordering::Relaxed);
                        }
                        client.next_seq = Some(header.seq.wrapping_add(1));
                        Verdict::Play
                    }
                }
                _ => Verdict::Reject,
            }
        };
        match verdict {
            Verdict::Play => {}
            Verdict::Drop => return,
            Verdict::Reject => {
                self.reject_unknown(from, header.session);
                return;
            }
        }

        self.shared.packets.fetch_add(1, Ordering::Relaxed);
        protocol::decode_pcm_s16le(payload, &mut self.pcm);

        let gain = self.shared.gain();
        let mut peak = 0.0f32;
        for s in self.pcm.iter_mut() {
            *s = (*s * gain).clamp(-1.0, 1.0);
            peak = peak.max(s.abs());
        }
        self.shared.record_peak(peak);
        self.shared.jitter.push(&self.pcm, header.sample_rate);
    }

    /// Refreshes the session if `session`/`from` match the active client.
    fn touch(&mut self, session: u32, from: SocketAddr) -> bool {
        let known = match self.shared.client.lock().unwrap().as_mut() {
            Some(c) if c.matches(session, from) => {
                c.addr = from;
                c.last_seen = Instant::now();
                true
            }
            _ => false,
        };
        if !known {
            self.reject_unknown(from, session);
        }
        known
    }

    /// Tells a phone whose session isn't active why: kicked/replaced (stop) or
    /// unknown (the desktop restarted; HELLO again).
    fn reject_unknown(&mut self, from: SocketAddr, session: u32) {
        let reason = self
            .shared
            .eviction_reason(session, from)
            .unwrap_or(RejectReason::UnknownSession);
        self.reject(from, session, reason);
    }

    fn reject(&mut self, to: SocketAddr, session: u32, reason: RejectReason) {
        if self.last_reject.get(&to).is_some_and(|t| t.elapsed() < REJECT_INTERVAL) {
            return;
        }
        prune(&mut self.last_reject, REJECT_INTERVAL);
        self.last_reject.insert(to, Instant::now());
        self.send(to, &Header::new(PacketType::Reject, session), &[reason as u8]);
    }

    fn send(&self, to: SocketAddr, header: &Header, payload: &[u8]) {
        let _ = self.socket.send_to(&protocol::encode(header, payload), to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> (Server, UdpSocket) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let phone = UdpSocket::bind("127.0.0.1:0").unwrap();
        phone.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        (Server::new(socket, Arc::new(Shared::new()), "PC".into()), phone)
    }

    fn hello(code: &str) -> Vec<u8> {
        protocol::encode(&Header::new(PacketType::Hello, 0), format!("{code}Phone").as_bytes())
    }

    fn audio(session: u32, rate: u32) -> Vec<u8> {
        audio_seq(session, rate, 0)
    }

    fn audio_seq(session: u32, rate: u32, seq: u32) -> Vec<u8> {
        let mut h = Header::new(PacketType::Audio, session);
        h.seq = seq;
        h.channels = 1;
        h.sample_rate = rate;
        protocol::encode(&h, &[0u8; 960])
    }

    /// Feeds `packet` from `from` and returns the reply, if any.
    fn exchange(server: &mut Server, phone: &UdpSocket, from: SocketAddr, packet: &[u8]) -> Option<(Header, Vec<u8>)> {
        server.handle(packet, from);
        let mut buf = [0u8; 2048];
        let (len, _) = phone.recv_from(&mut buf).ok()?;
        protocol::parse(&buf[..len]).ok().map(|(h, p)| (h, p.to_vec()))
    }

    fn welcome(server: &mut Server, phone: &UdpSocket, from: SocketAddr) -> u32 {
        let code = server.shared.code();
        let (h, _) = exchange(server, phone, from, &hello(&code)).expect("welcome");
        assert_eq!(h.kind, PacketType::Welcome);
        h.session
    }

    #[test]
    fn retried_hello_from_another_port_keeps_the_session() {
        let (mut server, phone) = server();
        let addr = phone.local_addr().unwrap();
        let first = welcome(&mut server, &phone, addr);
        // Same IP, different source port (iOS candidate race): same session.
        let other = UdpSocket::bind("127.0.0.1:0").unwrap();
        other.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        let other_addr = other.local_addr().unwrap();
        assert_eq!(welcome(&mut server, &other, other_addr), first);
        // ...and audio from either port is accepted.
        server.handle(&audio(first, 48_000), addr);
        server.handle(&audio_seq(first, 48_000, 1), other_addr);
        assert_eq!(server.shared.packets.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_new_phone_replaces_the_old_one_which_is_told_to_stop() {
        let (mut server, phone) = server();
        let addr = phone.local_addr().unwrap();
        let old = welcome(&mut server, &phone, addr);
        // Another device (different IP) pairs with the same code.
        let code = server.shared.code();
        server.handle(&hello(&code), "127.0.0.9:6000".parse().unwrap());
        let (h, payload) = exchange(&mut server, &phone, addr, &audio(old, 48_000)).unwrap();
        assert_eq!(h.kind, PacketType::Reject);
        assert_eq!(payload, [RejectReason::Replaced as u8]);
    }

    #[test]
    fn kicked_phone_is_told_to_stop() {
        let (mut server, phone) = server();
        let addr = phone.local_addr().unwrap();
        let session = welcome(&mut server, &phone, addr);
        server.shared.disconnect();
        let (h, payload) = exchange(&mut server, &phone, addr, &audio(session, 48_000)).unwrap();
        assert_eq!(h.kind, PacketType::Reject);
        assert_eq!(payload, [RejectReason::Kicked as u8]);
    }

    #[test]
    fn restarted_desktop_asks_for_hello_again() {
        let (mut server, phone) = server();
        let addr = phone.local_addr().unwrap();
        let (_, payload) = exchange(&mut server, &phone, addr, &audio(1234, 48_000)).unwrap();
        assert_eq!(payload, [RejectReason::UnknownSession as u8]);
    }

    #[test]
    fn wrong_codes_rotate_the_code() {
        let (mut server, phone) = server();
        let original = server.shared.code();
        let wrong = if original == "000000" { "111111" } else { "000000" };
        for i in 0..MAX_BAD_CODES {
            // Distinct IPs so the per-IP cooldown doesn't swallow attempts.
            let from: SocketAddr = format!("127.0.0.{}:5000", i + 2).parse().unwrap();
            server.handle(&hello(wrong), from);
        }
        assert_ne!(server.shared.code(), original);
        assert!(server.shared.code_rotated_for_safety());
        let _ = phone;
    }

    #[test]
    fn absurd_sample_rates_are_ignored() {
        let (mut server, phone) = server();
        let addr = phone.local_addr().unwrap();
        let session = welcome(&mut server, &phone, addr);
        server.handle(&audio(session, 0x8000_0000), addr);
        assert_eq!(server.shared.packets.load(Ordering::Relaxed), 0);
        server.handle(&audio(session, 48_000), addr);
        assert_eq!(server.shared.packets.load(Ordering::Relaxed), 1);
    }
}
