//! Client for the Celmic virtual microphone driver (drivers/windows).
//!
//! The driver exposes `\\.\CelmicVAD`; we push 48 kHz mono s16 PCM through an
//! IOCTL and it plays it out of the "Celmic Microphone" capture endpoint.
//! Mirrors drivers/windows/src/celmic_ioctl.h.

use std::io;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;

pub const SAMPLE_RATE: u32 = 48_000;
const DEVICE_PATH: &str = r"\\.\CelmicVAD";
const API_VERSION: u32 = 1;

const FILE_DEVICE_CELMIC: u32 = 0xC31C;
const METHOD_BUFFERED: u32 = 0;
const FILE_WRITE_DATA: u32 = 0x0002;

const fn ctl_code(device: u32, function: u32, method: u32, access: u32) -> u32 {
    (device << 16) | (access << 14) | (function << 2) | method
}

const IOCTL_CELMIC_PUSH: u32 = ctl_code(FILE_DEVICE_CELMIC, 0x800, METHOD_BUFFERED, FILE_WRITE_DATA);
const IOCTL_CELMIC_FLUSH: u32 = ctl_code(FILE_DEVICE_CELMIC, 0x801, METHOD_BUFFERED, FILE_WRITE_DATA);

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct DriverStatus {
    pub version: u32,
    pub sample_rate: u32,
    pub queued_frames: u32,
    pub capacity_frames: u32,
    pub active_streams: u32,
    pub overruns: u32,
    pub underruns: u32,
    reserved: u32,
}

/// Exclusive handle to the driver's control device.
pub struct VirtualMic {
    handle: HANDLE,
}

// The handle is only used through `&self` calls that the kernel serializes.
unsafe impl Send for VirtualMic {}

pub enum Presence {
    Available,
    /// Installed, but another process holds the (exclusive) handle.
    Busy,
    Missing,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl VirtualMic {
    pub fn open() -> io::Result<Self> {
        let path = wide(DEVICE_PATH);
        // SAFETY: valid NUL-terminated path; all other pointers are null/optional.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mic = Self { handle };
        let status = mic.push(&[])?;
        if status.version != API_VERSION || status.sample_rate != SAMPLE_RATE {
            return Err(io::Error::other(format!(
                "unsupported Celmic driver (api {}, {} Hz)",
                status.version, status.sample_rate
            )));
        }
        Ok(mic)
    }

    pub fn presence() -> Presence {
        match Self::open() {
            Ok(_) => Presence::Available,
            Err(e) if matches!(e.raw_os_error().map(|c| c as u32), Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION)) => {
                Presence::Busy
            }
            Err(_) => Presence::Missing,
        }
    }

    /// Queues `pcm` (may be empty) and returns the driver state after queuing.
    pub fn push(&self, pcm: &[i16]) -> io::Result<DriverStatus> {
        let mut status = DriverStatus::default();
        let mut returned = 0u32;
        // SAFETY: input/output buffers are valid for the given sizes for the
        // duration of this synchronous call.
        let ok = unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_CELMIC_PUSH,
                pcm.as_ptr().cast(),
                std::mem::size_of_val(pcm) as u32,
                (&mut status as *mut DriverStatus).cast(),
                std::mem::size_of::<DriverStatus>() as u32,
                &mut returned,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(status)
    }

    pub fn flush(&self) -> io::Result<()> {
        let mut returned = 0u32;
        // SAFETY: no buffers are passed.
        let ok = unsafe {
            DeviceIoControl(self.handle, IOCTL_CELMIC_FLUSH, null(), 0, null_mut(), 0, &mut returned, null_mut())
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for VirtualMic {
    fn drop(&mut self) {
        // SAFETY: handle came from CreateFileW and is closed exactly once.
        unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_codes_match_the_driver_header() {
        // CTL_CODE(0xC31C, 0x800, METHOD_BUFFERED, FILE_WRITE_DATA)
        assert_eq!(IOCTL_CELMIC_PUSH, 0xC31C_A000);
        assert_eq!(IOCTL_CELMIC_FLUSH, 0xC31C_A004);
        assert_eq!(std::mem::size_of::<DriverStatus>(), 32);
    }
}
