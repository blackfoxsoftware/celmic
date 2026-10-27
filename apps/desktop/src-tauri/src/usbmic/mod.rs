//! "Celmic Microphone" without a driver of our own (Windows).
//!
//! Celmic exports a USB Audio Class 2.0 microphone over USB/IP on localhost.
//! usbip-win2 (open source, Microsoft-signed drivers) attaches it, and Windows
//! binds its in-box usbaudio2.sys, so every app sees an ordinary USB mic.
//! Nothing needs test signing, and usbip-win2 installs with one UAC prompt.
//!
//! Adapted from Virtual Cables by Tom Albans (MIT License; see
//! LICENSE-virtual-cables): https://github.com/Tom56785/virtual-cables

mod clock;
mod protocol;
mod server;
pub mod setup;
mod uac2;

use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

pub use server::UsbMic;

use crate::server::Shared;

/// Where usbip-win2 reaches the microphone (localhost only).
pub const TCP_PORT: u16 = 47801;
pub const BUS_ID: &str = "1-1";

/// How often a detached microphone is attached again (e.g. after sleep).
const REATTACH_INTERVAL: Duration = Duration::from_secs(5);
const REATTACH_BACKOFF: Duration = Duration::from_secs(30);

static USB_MIC: OnceLock<UsbMic> = OnceLock::new();

/// The microphone, once its server is listening.
pub fn get() -> Option<&'static UsbMic> {
    USB_MIC.get()
}

/// Starts the server and keeps the microphone attached while usbip-win2 is
/// installed.
pub fn start(shared: Arc<Shared>) -> Result<(), String> {
    let mic = UsbMic::start(shared).map_err(|e| format!("could not listen on TCP {TCP_PORT}: {e}"))?;
    let _ = USB_MIC.set(mic);
    thread::Builder::new()
        .name("celmic-usbip-attach".into())
        .spawn(|| loop {
            let mut wait = REATTACH_INTERVAL;
            if let (Some(mic), Some(usbip)) = (get(), setup::usbip_exe()) {
                if !mic.attached() && setup::attach(&usbip).is_err() {
                    wait = REATTACH_BACKOFF;
                }
            }
            thread::sleep(wait);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
