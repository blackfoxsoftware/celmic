//! The USB bus clock as Windows USB audio sees it.
//!
//! usbaudio2.sys schedules isochronous streams against the bus frame number,
//! which usbip-win2 reports as KeQueryInterruptTime() / 10000. That value only
//! advances on each clock interrupt, so completions must be timed against the
//! same coarse value, or the host sees transfers on frames it believes have not
//! happened yet and resets the pipe.

use std::ffi::c_void;
use std::time::Duration;

use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION, PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling,
    SetProcessInformation,
};
use windows_sys::Win32::System::WindowsProgramming::{QueryInterruptTime, QueryInterruptTimePrecise};

/// 100 ns interrupt-time ticks per full-speed frame (1 ms).
pub const TICKS_PER_FRAME: u64 = 10_000;

/// The coarse interrupt time (what the host reads) and the precise one.
pub fn ticks() -> (u64, u64) {
    let (mut coarse, mut precise) = (0u64, 0u64);
    // SAFETY: both functions write one u64 through the pointer.
    unsafe {
        // Precise first, so coarse never appears ahead of it.
        QueryInterruptTimePrecise(&mut precise);
        QueryInterruptTime(&mut coarse);
    }
    (coarse.min(precise), precise)
}

/// The current bus frame, as usbaudio2.sys sees it.
pub fn frame() -> u64 {
    ticks().0 / TICKS_PER_FRAME
}

/// Time left until bus frame `frame` begins on the precise clock.
pub fn until_frame(frame: u64) -> Duration {
    let target = frame * TICKS_PER_FRAME;
    Duration::from_nanos(target.saturating_sub(ticks().1) * 100)
}

/// Keeps a 1 ms timer resolution while alive, so the coarse interrupt time
/// advances every frame like a real host controller's frame counter.
///
/// Windows 11 ignores timer requests from minimized or hidden windows unless
/// the process opts out of power throttling, which this also does.
pub struct TimerResolution(());

impl TimerResolution {
    pub fn acquire() -> TimerResolution {
        // SAFETY: plain calls on the current-process pseudo handle with a
        // correctly sized local structure.
        unsafe {
            let state = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
                StateMask: 0, // never throttle
            };
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                (&state as *const PROCESS_POWER_THROTTLING_STATE).cast::<c_void>(),
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
            timeBeginPeriod(1);
        }
        TimerResolution(())
    }
}

impl Drop for TimerResolution {
    fn drop(&mut self) {
        // SAFETY: matches the timeBeginPeriod in `acquire`.
        unsafe { timeEndPeriod(1) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coarse_clock_tracks_the_precise_one() {
        let _resolution = TimerResolution::acquire();
        let start = ticks();
        std::thread::sleep(Duration::from_millis(30));
        let end = ticks();
        assert!(end.1 - start.1 >= 30 * TICKS_PER_FRAME);
        assert!(end.1 - end.0 <= 16 * TICKS_PER_FRAME, "coarse lags by at most one timer tick");
    }

    #[test]
    fn until_frame_counts_down() {
        let _resolution = TimerResolution::acquire();
        std::thread::sleep(Duration::from_millis(20)); // let the faster tick take effect
        let next = frame() + 5;
        let left = until_frame(next);
        assert!(left > Duration::ZERO && left <= Duration::from_millis(6), "{left:?}");
        assert_eq!(until_frame(frame().saturating_sub(5)), Duration::ZERO);
    }
}
