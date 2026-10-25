//! Installs / removes the Celmic virtual microphone driver (Windows).
//!
//! It is a root-enumerated device, so installing means what `devcon install`
//! does: create the `ROOT\CelmicVAD` device node, then bind the INF to it.
//! These calls need administrator rights, so the app re-launches itself
//! elevated with `--install-driver <inf>` / `--uninstall-driver <inf>`.

use std::io;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DI_NEEDREBOOT, DI_NEEDRESTART, DICD_GENERATE_ID, DIF_REGISTERDEVICE, DIGCF_ALLCLASSES, DIGCF_PRESENT, DiUninstallDriverW, HDEVINFO,
    INSTALLFLAG_FORCE, SP_DEVINFO_DATA, SPDRP_HARDWAREID, SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList,
    SetupDiCreateDeviceInfoW, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceInstallParamsW, SetupDiGetDeviceRegistryPropertyW, SP_DEVINSTALL_PARAMS_W, SetupDiGetINFClassW, SetupDiRemoveDevice, SetupDiSetDeviceRegistryPropertyW,
    UpdateDriverForPlugAndPlayDevicesW,
};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NO_CONSOLE, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::core::GUID;

pub const HARDWARE_ID: &str = r"ROOT\CelmicVAD";
const INF_NAME: &str = "CelmicVAD.inf";
const INVALID_HDEVINFO: HDEVINFO = -1;

/// Exit codes of the elevated helper.
pub const EXIT_OK: i32 = 0;
pub const EXIT_OK_REBOOT: i32 = 3010; // ERROR_SUCCESS_REBOOT_REQUIRED
pub const EXIT_FAILED: i32 = 1;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_path(p: &Path) -> Vec<u16> {
    wide(&p.to_string_lossy())
}

fn last_error(context: &str) -> String {
    // SAFETY: trivially safe.
    let code = unsafe { GetLastError() };
    format!("{context}: {}", io::Error::from_raw_os_error(code as i32))
}

/// Where the driver package lives: bundled next to the app, overridden by
/// `CELMIC_DRIVER_DIR`, or the local build output in a dev checkout.
pub fn find_inf(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("CELMIC_DRIVER_DIR") {
        candidates.push(PathBuf::from(dir).join(INF_NAME));
    }
    if let Some(dir) = resource_dir {
        candidates.push(dir.join("driver").join(INF_NAME));
    }
    let arch = if cfg!(target_arch = "aarch64") { "ARM64" } else { "x64" };
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../drivers/windows/build").join(arch);
    candidates.push(dev.join("Release/CelmicVAD").join(INF_NAME));
    candidates.push(dev.join("Debug/CelmicVAD").join(INF_NAME));
    let found = candidates.into_iter().find(|p| p.is_file())?.canonicalize().ok()?;
    // SetupAPI does not accept verbatim (\\?\) paths.
    let plain = found.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    Some(PathBuf::from(plain))
}

/// Runs this executable elevated (UAC prompt) with `args` and waits for it.
pub fn run_elevated(args: &[&str]) -> Result<i32, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let params = args
        .iter()
        .map(|a| format!("\"{a}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let (verb, file, params) = (wide("runas"), wide_path(&exe), wide(&params));

    // SAFETY: the struct is plain data; every pointer stays alive for the call.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NO_CONSOLE;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = 0; // SW_HIDE

        if ShellExecuteExW(&mut info) == 0 {
            return Err(last_error("could not start the elevated installer (UAC declined?)"));
        }
        if info.hProcess.is_null() {
            return Err("elevated installer did not start".into());
        }
        let wait = WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = EXIT_FAILED as u32;
        let ok = wait == WAIT_OBJECT_0 && GetExitCodeProcess(info.hProcess, &mut code) != 0;
        CloseHandle(info.hProcess);
        if !ok {
            return Err("lost track of the elevated installer".into());
        }
        Ok(code as i32)
    }
}

/// RAII wrapper for a SetupAPI device information set.
struct DevInfoSet(HDEVINFO);

impl Drop for DevInfoSet {
    fn drop(&mut self) {
        // SAFETY: created by SetupDi* and destroyed once.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

/// Calls `f` for every present device whose hardware IDs contain ours.
fn for_each_celmic_device(mut f: impl FnMut(HDEVINFO, &mut SP_DEVINFO_DATA)) -> Result<usize, String> {
    let root = wide("ROOT");
    // SAFETY: standard SetupAPI enumeration; buffers are sized explicitly.
    unsafe {
        let set = SetupDiGetClassDevsW(null(), root.as_ptr(), null_mut(), DIGCF_ALLCLASSES | DIGCF_PRESENT);
        if set == INVALID_HDEVINFO {
            return Err(last_error("SetupDiGetClassDevs"));
        }
        let set = DevInfoSet(set);
        let mut found = 0;
        let mut index = 0;
        loop {
            let mut data: SP_DEVINFO_DATA = zeroed();
            data.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
            if SetupDiEnumDeviceInfo(set.0, index, &mut data) == 0 {
                break;
            }
            index += 1;

            let mut buf = [0u16; 512];
            let ok = SetupDiGetDeviceRegistryPropertyW(
                set.0,
                &data,
                SPDRP_HARDWAREID,
                null_mut(),
                buf.as_mut_ptr().cast(),
                (buf.len() * 2) as u32,
                null_mut(),
            );
            if ok == 0 {
                continue;
            }
            let ids = String::from_utf16_lossy(&buf);
            if ids.split('\0').any(|id| id.eq_ignore_ascii_case(HARDWARE_ID)) {
                found += 1;
                f(set.0, &mut data);
            }
        }
        Ok(found)
    }
}

pub fn is_device_present() -> bool {
    for_each_celmic_device(|_, _| {}).map(|n| n > 0).unwrap_or(false)
}

/// Creates the root device (if needed) and installs the driver on it.
/// Returns whether a reboot is required. Requires administrator rights.
pub fn install(inf: &Path) -> Result<bool, String> {
    let inf_w = wide_path(inf);
    let hwid = wide(HARDWARE_ID);
    let created = !is_device_present();

    // SAFETY: SetupAPI calls with correctly sized, live buffers.
    unsafe {
        if created {
            let mut class_guid: GUID = zeroed();
            let mut class_name = [0u16; 32];
            if SetupDiGetINFClassW(inf_w.as_ptr(), &mut class_guid, class_name.as_mut_ptr(), 32, null_mut()) == 0 {
                return Err(last_error("SetupDiGetINFClass"));
            }

            let set = SetupDiCreateDeviceInfoList(&class_guid, null_mut());
            if set == INVALID_HDEVINFO {
                return Err(last_error("SetupDiCreateDeviceInfoList"));
            }
            let set = DevInfoSet(set);

            let mut data: SP_DEVINFO_DATA = zeroed();
            data.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
            if SetupDiCreateDeviceInfoW(
                set.0,
                class_name.as_ptr(),
                &class_guid,
                null(),
                null_mut(),
                DICD_GENERATE_ID,
                &mut data,
            ) == 0
            {
                return Err(last_error("SetupDiCreateDeviceInfo"));
            }

            // REG_MULTI_SZ: "ROOT\CelmicVAD\0\0"
            let mut multi = hwid.clone();
            multi.push(0);
            if SetupDiSetDeviceRegistryPropertyW(
                set.0,
                &mut data,
                SPDRP_HARDWAREID,
                multi.as_ptr().cast(),
                (multi.len() * 2) as u32,
            ) == 0
            {
                return Err(last_error("SetupDiSetDeviceRegistryProperty"));
            }

            if SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set.0, &data) == 0 {
                return Err(last_error("registering the device"));
            }
        }

        let mut reboot = 0;
        if UpdateDriverForPlugAndPlayDevicesW(null_mut(), hwid.as_ptr(), inf_w.as_ptr(), INSTALLFLAG_FORCE, &mut reboot)
            == 0
        {
            let err = last_error("installing the driver");
            if created {
                let _ = remove_devices();
            }
            return Err(err);
        }
        Ok(reboot != 0)
    }
}

/// Removes every Celmic device node. Returns whether Windows needs a reboot to
/// finish (e.g. an app still had the microphone open, which vetoes removal).
fn remove_devices() -> Result<bool, String> {
    let mut failures = Vec::new();
    let mut reboot = false;
    for_each_celmic_device(|set, data| {
        // SAFETY: `set`/`data` come from the live enumeration; `params` is sized.
        unsafe {
            if SetupDiRemoveDevice(set, data) == 0 {
                failures.push(last_error("SetupDiRemoveDevice"));
                return;
            }
            let mut params: SP_DEVINSTALL_PARAMS_W = zeroed();
            params.cbSize = size_of::<SP_DEVINSTALL_PARAMS_W>() as u32;
            if SetupDiGetDeviceInstallParamsW(set, data, &mut params) != 0
                && params.Flags & (DI_NEEDREBOOT | DI_NEEDRESTART) != 0
            {
                reboot = true;
            }
        }
    })?;
    match failures.pop() {
        Some(e) => Err(e),
        None => Ok(reboot),
    }
}

/// Removes the device and the driver package. Requires administrator rights.
pub fn uninstall(inf: &Path) -> Result<bool, String> {
    let device_reboot = remove_devices()?;
    let mut package_reboot = 0;
    let inf_w = wide_path(inf);
    // SAFETY: valid NUL-terminated path.
    if unsafe { DiUninstallDriverW(null_mut(), inf_w.as_ptr(), 0, &mut package_reboot) } == 0 {
        return Err(last_error("removing the driver package"));
    }
    Ok(device_reboot || package_reboot != 0)
}

const INSTALL_FLAG: &str = "--install-driver";
const UNINSTALL_FLAG: &str = "--uninstall-driver";

/// Entry point for the elevated helper: `<flag> <inf> <error-log>`.
/// Returns a process exit code, or `None` when the args aren't ours.
pub fn run_cli(args: &[String]) -> Option<i32> {
    let [flag, inf, log, ..] = args else {
        return None;
    };
    let result = match flag.as_str() {
        INSTALL_FLAG => install(Path::new(inf)),
        UNINSTALL_FLAG => uninstall(Path::new(inf)),
        _ => return None,
    };
    Some(match result {
        Ok(true) => EXIT_OK_REBOOT,
        Ok(false) => EXIT_OK,
        Err(e) => {
            // The elevated process has no console; hand the message back via a file.
            let _ = std::fs::write(log, e);
            EXIT_FAILED
        }
    })
}

/// Installs (or removes) the driver through an elevated copy of this app.
/// Returns whether Windows asked for a reboot.
pub fn elevated(install: bool, inf: &Path) -> Result<bool, String> {
    let log = std::env::temp_dir().join(format!("celmic-driver-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log);
    let flag = if install { INSTALL_FLAG } else { UNINSTALL_FLAG };
    let code = run_elevated(&[flag, &inf.to_string_lossy(), &log.to_string_lossy()])?;
    let message = std::fs::read_to_string(&log).ok();
    let _ = std::fs::remove_file(&log);
    match code {
        EXIT_OK => Ok(false),
        EXIT_OK_REBOOT => Ok(true),
        _ => Err(message.unwrap_or_else(|| format!("driver setup failed (exit code {code})"))),
    }
}
