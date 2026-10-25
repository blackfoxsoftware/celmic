//! One-click virtual microphone on Linux (PulseAudio / PipeWire-pulse).
//!
//! Creates a null sink `celmic` (which we play into) and remaps its monitor as a
//! proper source `celmic_mic`, so apps list "Celmic Microphone" as an input.
//! Windows and macOS need a third-party driver (VB-CABLE / BlackHole) for now.

#[cfg(target_os = "linux")]
pub fn create() -> Result<(), String> {
    use std::process::Command;

    fn pactl(args: &[&str]) -> Result<String, String> {
        let out = Command::new("pactl")
            .args(args)
            .output()
            .map_err(|e| format!("could not run pactl ({e}); install pulseaudio-utils"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    let has = |kind: &str, name: &str| -> Result<bool, String> {
        Ok(pactl(&["list", "short", kind])?
            .lines()
            .any(|line| line.split('\t').nth(1) == Some(name)))
    };

    if !has("sinks", "celmic")? {
        pactl(&[
            "load-module",
            "module-null-sink",
            "sink_name=celmic",
            "sink_properties=device.description=Celmic",
        ])?;
    }
    if !has("sources", "celmic_mic")? {
        pactl(&[
            "load-module",
            "module-remap-source",
            "master=celmic.monitor",
            "source_name=celmic_mic",
            "source_properties=device.description=Celmic_Microphone",
        ])?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn create() -> Result<(), String> {
    Err(if cfg!(target_os = "windows") {
        "Install VB-CABLE (https://vb-audio.com/Cable/) and pick \"CABLE Input\" as output.".into()
    } else {
        "Install BlackHole (https://existential.audio/blackhole/) and pick it as output.".into()
    })
}
