mod discovery;
#[cfg(windows)]
pub mod driver_install;
mod jitter;
mod output;
mod protocol;
mod server;
#[cfg(windows)]
mod vad;
mod virtual_mic;

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, Manager, State};

use output::{ActiveOutput, DeviceInfo, Output, VirtualMicInfo};
use server::{ClientInfo, Shared};

const DEFAULT_PORT: u16 = 47800;
const STATUS_INTERVAL: Duration = Duration::from_millis(80);
/// Status ticks between LAN address checks (~2.4 s).
const IP_CHECK_TICKS: u32 = 30;

struct AppState {
    shared: Arc<Shared>,
    output: Output,
    name: String,
    port: u16,
    /// Current LAN addresses; refreshed periodically (Wi-Fi may connect late).
    ips: Mutex<Vec<Ipv4Addr>>,
    listen_error: Option<String>,
    mdns_error: Option<String>,
    mdns: Option<discovery::Advertiser>,
}

impl AppState {
    fn ips(&self) -> Vec<String> {
        self.ips.lock().unwrap().iter().map(ToString::to_string).collect()
    }

    /// Picks up network changes and re-announces over mDNS.
    fn refresh_ips(&self) {
        let ips = discovery::lan_ipv4s();
        let mut current = self.ips.lock().unwrap();
        if *current != ips {
            if let Some(mdns) = &self.mdns {
                let _ = mdns.announce(&ips);
            }
            *current = ips;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    name: String,
    port: u16,
    code: String,
    ips: Vec<String>,
    version: &'static str,
    listen_error: Option<String>,
    mdns_error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    /// Pairing code and addresses, live so the QR code follows changes.
    code: String,
    code_rotated_for_safety: bool,
    ips: Vec<String>,
    client: Option<ClientInfo>,
    /// Peak level (0..1) since the previous status event.
    level: f32,
    packets: u64,
    lost: u64,
    underruns: u64,
    buffer_ms: f32,
    target_latency_ms: u32,
    gain: f32,
    output: Option<ActiveOutput>,
    output_error: Option<String>,
    /// Set while the Celmic driver is the active output (Windows).
    virtual_mic: Option<VirtualMicInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DriverState {
    /// The platform has a native Celmic driver at all (Windows).
    supported: bool,
    installed: bool,
    /// A driver package to install was found (bundled or built locally).
    package_available: bool,
}

#[tauri::command]
fn get_info(state: State<AppState>) -> Info {
    Info {
        name: state.name.clone(),
        port: state.port,
        code: state.shared.code(),
        ips: state.ips(),
        version: env!("CARGO_PKG_VERSION"),
        listen_error: state.listen_error.clone(),
        mdns_error: state.mdns_error.clone(),
    }
}

#[tauri::command]
fn regenerate_code(state: State<AppState>) -> String {
    state.shared.regenerate_code()
}

#[tauri::command]
fn list_output_devices() -> Vec<DeviceInfo> {
    output::list_devices()
}

/// Runs blocking work (device open, pactl, UAC) off the async runtime.
async fn blocking<T: Send + 'static>(
    app: tauri::AppHandle,
    f: impl FnOnce(&AppState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_output_device(app: tauri::AppHandle, id: Option<String>) -> Result<ActiveOutput, String> {
    blocking(app, move |state| state.output.select(id)).await
}

#[tauri::command]
fn set_gain(state: State<AppState>, gain: f32) {
    state.shared.set_gain(gain);
}

#[tauri::command]
fn set_latency(state: State<AppState>, ms: u32) {
    state.shared.target_latency_ms.store(ms.clamp(10, 500), Ordering::Relaxed);
}

#[tauri::command]
fn disconnect_client(state: State<AppState>) {
    state.shared.disconnect();
}

#[tauri::command]
async fn create_virtual_mic(app: tauri::AppHandle) -> Result<ActiveOutput, String> {
    blocking(app, |state| {
        virtual_mic::create()?;
        let device = output::list_devices()
            .into_iter()
            .find(|d| d.name.to_lowercase().contains("celmic"))
            .ok_or("virtual sink created but not visible yet; refresh the device list")?;
        state.output.select(Some(device.id))
    })
    .await
}

#[tauri::command]
fn driver_state(app: tauri::AppHandle) -> DriverState {
    #[cfg(windows)]
    {
        let resources = app.path().resource_dir().ok();
        DriverState {
            supported: true,
            installed: output::virtual_mic_installed(),
            package_available: driver_install::find_inf(resources.as_deref()).is_some(),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        DriverState { supported: false, installed: false, package_available: false }
    }
}

/// Installs (or removes) the Windows driver via an elevated helper (UAC prompt),
/// then switches the output accordingly. Returns whether a reboot was requested.
#[tauri::command]
async fn set_driver_installed(app: tauri::AppHandle, install: bool) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let resources = app.path().resource_dir().ok();
        let inf = driver_install::find_inf(resources.as_deref())
            .ok_or("driver package not found; build it with `bun run driver:build`")?;
        blocking(app, move |state| {
            if !install {
                // Our own handle to \.CelmicVAD would veto the removal.
                state.output.release();
            }
            let result = driver_install::elevated(install, &inf);

            // Switch to the new microphone, or back to automatic after removal.
            let target = (install && result.is_ok()).then(|| output::VIRTUAL_MIC_ID.to_string());
            let selected = state.output.select(target);
            let reboot = result?;
            if let Err(e) = selected {
                if install && !reboot {
                    return Err(format!("driver installed, but the microphone did not start yet: {e}"));
                }
            }
            Ok(reboot)
        })
        .await
    }
    #[cfg(not(windows))]
    {
        let _ = (app, install);
        Err("the Celmic driver is only available on Windows".into())
    }
}

fn status(state: &AppState) -> Status {
    let shared = &state.shared;
    Status {
        code: shared.code(),
        code_rotated_for_safety: shared.code_rotated_for_safety(),
        ips: state.ips(),
        client: shared.client_info(),
        level: shared.take_peak(),
        packets: shared.packets.load(Ordering::Relaxed),
        lost: shared.lost.load(Ordering::Relaxed),
        underruns: shared.jitter.underruns(),
        buffer_ms: shared.jitter.fill_ms(),
        target_latency_ms: shared.target_latency_ms.load(Ordering::Relaxed),
        gain: shared.gain(),
        output: state.output.active(),
        output_error: state.output.error(),
        virtual_mic: state.output.virtual_mic(),
    }
}

fn desktop_name() -> String {
    let name = gethostname::gethostname().to_string_lossy().trim().to_string();
    if name.is_empty() { "Celmic Desktop".into() } else { name }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let port = std::env::var("CELMIC_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_PORT);
            let name = desktop_name();
            let shared = Arc::new(Shared::new());

            let listen_error = server::spawn(shared.clone(), port, name.clone())
                .err()
                .map(|e| format!("could not listen on UDP {port}: {e}"));
            let ips = discovery::lan_ipv4s();
            let (mdns, mdns_error) = match discovery::Advertiser::start(&name, port, &ips) {
                Ok(advertiser) => (Some(advertiser), None),
                Err(e) => (None, Some(e)),
            };
            let output = Output::spawn(shared.clone());

            app.manage(AppState {
                shared,
                output,
                name,
                port,
                ips: Mutex::new(ips),
                listen_error,
                mdns_error,
                mdns,
            });

            let handle = app.handle().clone();
            thread::Builder::new()
                .name("celmic-status".into())
                .spawn(move || {
                    let mut tick = 0u32;
                    loop {
                        let state = handle.state::<AppState>();
                        if tick % IP_CHECK_TICKS == 0 {
                            state.refresh_ips();
                        }
                        tick = tick.wrapping_add(1);
                        let _ = handle.emit("status", status(&state));
                        thread::sleep(STATUS_INTERVAL);
                    }
                })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_info,
            regenerate_code,
            list_output_devices,
            set_output_device,
            set_gain,
            set_latency,
            disconnect_client,
            create_virtual_mic,
            driver_state,
            set_driver_installed,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
