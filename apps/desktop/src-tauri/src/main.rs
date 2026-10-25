// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Elevated helper mode: `--install-driver <inf> <log>` / `--uninstall-driver <inf> <log>`.
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if let Some(code) = celmic_lib::driver_install::run_cli(&args) {
            std::process::exit(code);
        }
    }
    celmic_lib::run()
}
