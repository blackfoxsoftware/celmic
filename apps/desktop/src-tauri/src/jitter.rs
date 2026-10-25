//! Adaptive jitter buffer + drift-compensating resampler.
//!
//! The network thread pushes mono f32 samples at the phone's rate. The audio
//! output callback pulls through a [`Playout`] which resamples to the device
//! rate and gently speeds up / slows down to hold the buffer at its target
//! latency. That absorbs Wi-Fi jitter and the clock drift between the phone's
//! ADC and the PC's DAC without periodic clicks.

use std::collections::VecDeque;
use std::sync::Mutex;

/// Maximum playback-rate correction applied to track the target fill (±0.3 %).
const MAX_DRIFT_CORRECTION: f64 = 0.003;
/// Proportional gain of the fill-level controller.
const DRIFT_GAIN: f64 = 0.004;
/// Smoothing for the fill-level estimate (per output callback).
const FILL_EMA: f64 = 0.05;
/// Hard cap on queued audio when nothing consumes it.
const MAX_BUFFER_SECS: u32 = 2;

pub struct JitterBuffer {
    inner: Mutex<Inner>,
}

struct Inner {
    queue: VecDeque<f32>,
    in_rate: u32,
    /// True until the queue reaches the target after a reset / underrun.
    buffering: bool,
    underruns: u64,
    /// Bumped on clear so the playout side can reset its interpolation state.
    generation: u64,
}

impl JitterBuffer {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                queue: VecDeque::with_capacity(48_000),
                in_rate: 48_000,
                buffering: true,
                underruns: 0,
                generation: 0,
            }),
        }
    }

    pub fn push(&self, samples: &[f32], rate: u32) {
        let mut g = self.inner.lock().unwrap();
        if rate != g.in_rate && rate > 0 {
            g.queue.clear();
            g.in_rate = rate;
            g.buffering = true;
            g.generation += 1;
        }
        g.queue.extend(samples.iter().copied());

        // Nobody is pulling (no output open, or no app recording from the
        // virtual mic): keep memory bounded. Playout trims to target anyway.
        let max = g.in_rate as usize * MAX_BUFFER_SECS as usize;
        if g.queue.len() > max {
            let excess = g.queue.len() - max;
            g.queue.drain(..excess);
        }
    }

    pub fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.queue.clear();
        g.buffering = true;
        g.generation += 1;
    }

    /// Current fill in milliseconds of input audio.
    pub fn fill_ms(&self) -> f32 {
        let g = self.inner.lock().unwrap();
        g.queue.len() as f32 * 1000.0 / g.in_rate as f32
    }

    pub fn underruns(&self) -> u64 {
        self.inner.lock().unwrap().underruns
    }
}

/// Output-side state. Lives inside the audio callback.
pub struct Playout {
    out_rate: u32,
    prev: f32,
    cur: f32,
    /// Fractional read position between `prev` and `cur`.
    frac: f64,
    avg_fill: f64,
    generation: u64,
}

impl Playout {
    pub fn new(out_rate: u32) -> Self {
        Self {
            out_rate,
            prev: 0.0,
            cur: 0.0,
            frac: 1.0,
            avg_fill: 0.0,
            generation: u64::MAX,
        }
    }

    fn reset(&mut self) {
        self.prev = 0.0;
        self.cur = 0.0;
        self.frac = 1.0;
    }

    /// Fills `out` (mono) with audio from `jb`, keeping roughly `target_ms` buffered.
    pub fn fill(&mut self, jb: &JitterBuffer, target_ms: u32, out: &mut [f32]) {
        let mut g = jb.inner.lock().unwrap();

        if g.generation != self.generation {
            self.generation = g.generation;
            self.reset();
        }

        let in_rate = g.in_rate as f64;
        let target = (in_rate * target_ms as f64 / 1000.0).max(1.0);

        if g.buffering {
            if (g.queue.len() as f64) < target {
                out.fill(0.0);
                return;
            }
            g.buffering = false;
            self.avg_fill = g.queue.len() as f64;
        }

        // Way too much audio queued (e.g. Wi-Fi burst after a stall): jump back to target.
        let max = target * 2.0 + in_rate * 0.05;
        if g.queue.len() as f64 > max {
            let excess = g.queue.len() - target as usize;
            g.queue.drain(..excess);
            self.avg_fill = target;
        }

        self.avg_fill += (g.queue.len() as f64 - self.avg_fill) * FILL_EMA;
        let error = (self.avg_fill - target) / target;
        let correction = (error * DRIFT_GAIN).clamp(-MAX_DRIFT_CORRECTION, MAX_DRIFT_CORRECTION);
        let step = in_rate / self.out_rate as f64 * (1.0 + correction);

        for i in 0..out.len() {
            while self.frac >= 1.0 {
                match g.queue.pop_front() {
                    Some(sample) => {
                        self.prev = self.cur;
                        self.cur = sample;
                        self.frac -= 1.0;
                    }
                    None => {
                        g.buffering = true;
                        g.underruns += 1;
                        self.reset();
                        out[i..].fill(0.0);
                        return;
                    }
                }
            }
            out[i] = self.prev + (self.cur - self.prev) * self.frac as f32;
            self.frac += step;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_target_then_plays() {
        let jb = JitterBuffer::new();
        let mut playout = Playout::new(48_000);
        let mut out = vec![1.0; 480];

        jb.push(&vec![0.5; 960], 48_000); // 20 ms < 40 ms target
        playout.fill(&jb, 40, &mut out);
        assert!(out.iter().all(|&s| s == 0.0));

        jb.push(&vec![0.5; 1920], 48_000); // now 60 ms
        playout.fill(&jb, 40, &mut out);
        // after the first interpolated sample everything is 0.5
        assert!(out[2..].iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn underrun_rebuffers() {
        let jb = JitterBuffer::new();
        let mut playout = Playout::new(48_000);
        jb.push(&vec![0.5; 1920], 48_000);
        let mut out = vec![0.0; 4000];
        playout.fill(&jb, 40, &mut out);
        assert_eq!(jb.underruns(), 1);
        assert!(out[3000..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn resamples_to_device_rate() {
        let jb = JitterBuffer::new();
        let mut playout = Playout::new(44_100);
        jb.push(&vec![0.25; 48_000], 48_000);
        let mut out = vec![0.0; 882]; // 20 ms at 44.1k
        playout.fill(&jb, 40, &mut out);
        assert!(out[2..].iter().all(|&s| (s - 0.25).abs() < 1e-6));
        // 1 s pushed exceeds the overflow ceiling, so it was trimmed to the 40 ms
        // target first, then ~20 ms were consumed.
        let fill = jb.fill_ms();
        assert!(fill > 15.0 && fill < 25.0, "fill = {fill}");
        assert_eq!(jb.underruns(), 0);
    }

    #[test]
    fn push_is_bounded_without_a_consumer() {
        let jb = JitterBuffer::new();
        for _ in 0..1000 {
            jb.push(&[0.0; 480], 48_000); // 10 s of audio
        }
        assert!(jb.fill_ms() <= MAX_BUFFER_SECS as f32 * 1000.0);
    }

    #[test]
    fn rate_change_resets() {
        let jb = JitterBuffer::new();
        jb.push(&vec![0.1; 4800], 48_000);
        jb.push(&vec![0.1; 441], 44_100);
        assert!((jb.fill_ms() - 10.0).abs() < 0.01);
    }
}
