//! Installs usbip-win2 and attaches the microphone with its usbip.exe.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows_sys::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};

use super::{BUS_ID, TCP_PORT};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The usbip-win2 release we install: pinned, and checked against its SHA-256.
struct Installer {
    url: &'static str,
    sha256: &'static str,
}

#[cfg(target_arch = "x86_64")]
const INSTALLER: Installer = Installer {
    url: "https://github.com/vadimgrn/usbip-win2/releases/download/v.0.9.8.1/USBip-0.9.8.1-x64.exe",
    sha256: "38cad6d4432b52d5bb9409d9ad03b72fdffc4ada4cd3a48fbeca1a2752a8518a",
};
#[cfg(target_arch = "aarch64")]
const INSTALLER: Installer = Installer {
    url: "https://github.com/vadimgrn/usbip-win2/releases/download/v.0.9.8.1/USBip-0.9.8.1-arm64.exe",
    sha256: "cab7ff97f79275eeb5c5b8bb2eb3111ff6b6c4eead07d9bec9be5bd1e3a35800",
};

/// Inno Setup exit code we ask for when Windows must restart.
const EXIT_REBOOT: i32 = 3010;

/// usbip.exe of an installed usbip-win2.
pub fn usbip_exe() -> Option<PathBuf> {
    let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from)?;
    ["USBip", "usbip-win2"]
        .iter()
        .map(|dir| program_files.join(dir).join("usbip.exe"))
        .find(|path| path.is_file())
}

fn run(exe: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("{}: {e}", exe.display()))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = text.trim().to_string();
    if output.status.success() { Ok(text) } else { Err(text) }
}

/// The microphone's URL in `usbip port` output.
fn attached_url() -> String {
    format!("usbip://127.0.0.1:{TCP_PORT}/{BUS_ID}")
}

/// Attaches the microphone to Windows, unless it already is. No elevation
/// is needed: usbip-win2 lets users attach devices.
///
/// Without `--once`, usbip-win2 keeps reattaching by itself, so the
/// microphone also comes back when Celmic restarts. With usbip-win2 0.9.8.1,
/// `--once` attachments (and `--receive-mode low-latency`) dropped the
/// connection when an app stopped recording.
pub fn attach(usbip: &Path) -> Result<(), String> {
    if run(usbip, &["port"]).is_ok_and(|ports| ports.contains(&attached_url())) {
        return Ok(());
    }
    let port = TCP_PORT.to_string();
    let target = ["--tcp-port", &port, "attach", "-r", "127.0.0.1", "-b", BUS_ID];
    // Drop attempts still pending from an earlier run, so there is only one.
    let _ = run(usbip, &[&target[..], &["--stop"]].concat());
    run(usbip, &target)
        .map(drop)
        .map_err(|e| format!("could not attach Celmic Microphone: {e}"))
}

fn sha256(data: &[u8]) -> String {
    let mut digest = [0u8; 32];
    // SAFETY: the pseudo algorithm handle needs no setup; the buffers are
    // valid for the given lengths.
    let status = unsafe {
        BCryptHash(BCRYPT_SHA256_ALG_HANDLE, std::ptr::null(), 0, data.as_ptr(), data.len() as u32, digest.as_mut_ptr(), 32)
    };
    if status != 0 {
        return String::new();
    }
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads the pinned usbip-win2 installer into `dir` and verifies it.
pub fn download_installer(dir: &Path) -> Result<PathBuf, String> {
    let file = dir.join(INSTALLER.url.rsplit('/').next().unwrap_or("usbip-setup.exe"));
    // curl.exe ships with Windows 10 1803 and later.
    let curl = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
        .join(r"System32\curl.exe");
    run(&curl, &["-fsSL", "--retry", "2", "-o", &file.to_string_lossy(), INSTALLER.url])
        .map_err(|e| format!("could not download usbip-win2: {e}"))?;
    let data = std::fs::read(&file).map_err(|e| e.to_string())?;
    if sha256(&data) != INSTALLER.sha256 {
        let _ = std::fs::remove_file(&file);
        return Err("the downloaded usbip-win2 installer did not match its checksum".into());
    }
    Ok(file)
}

/// Runs the installer elevated (one UAC prompt). Returns whether Windows
/// asked for a restart.
pub fn install(installer: &Path) -> Result<bool, String> {
    let restart_code = format!("/RESTARTEXITCODE={EXIT_REBOOT}");
    let code = crate::driver_install::run_exe_elevated(
        installer,
        &["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", &restart_code],
    )?;
    match code {
        0 => Ok(false),
        EXIT_REBOOT => Ok(true),
        code => Err(format!("usbip-win2 setup failed (exit code {code})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_a_known_digest() {
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn installer_is_pinned() {
        assert!(INSTALLER.url.starts_with("https://github.com/vadimgrn/usbip-win2/releases/download/"));
        assert_eq!(INSTALLER.sha256.len(), 64);
    }
}
