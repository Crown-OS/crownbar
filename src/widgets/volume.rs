//! Volume widget. Polls `wpctl` (PipeWire) for the default sink, falls back
//! to `pactl` (PulseAudio). We avoid linking libpulse/libpipewire — running
//! a short subprocess every 2 seconds is cheap and works on any audio stack
//! that ships either tool, which is essentially every modern desktop.

use std::process::Command;

use crate::{
    animation::Spring,
    util::poll::PollGate,
    widgets::{BarWidget, Icon, WidgetSlot},
};

const POLL_PERIOD_TICKS: u32 = 2;

pub struct VolumeWidget {
    backend: Backend,
    muted: bool,
    level: Spring,
    muted_anim: Spring,
    gate: PollGate,
}

#[derive(Copy, Clone)]
enum Backend {
    Wpctl,
    Pactl,
}

impl VolumeWidget {
    pub fn try_new() -> Option<Self> {
        let backend = detect_backend()?;
        let mut w = Self {
            backend,
            muted: false,
            level: Spring::new(0.0),
            muted_anim: Spring::new(0.0),
            gate: PollGate::new(POLL_PERIOD_TICKS),
        };
        w.refresh();
        w.level.position = w.level.target;
        w.muted_anim.position = w.muted_anim.target;
        Some(w)
    }

    fn refresh(&mut self) -> bool {
        let reading = match self.backend {
            Backend::Wpctl => read_wpctl(),
            Backend::Pactl => read_pactl(),
        };
        let Some(r) = reading else {
            return false;
        };
        let next_muted = r.muted;
        let next_level = r.volume as f32 / 100.0;
        let changed = (next_level - self.level.target).abs() > 0.005 || next_muted != self.muted;
        self.muted = next_muted;
        self.level.set_target(next_level);
        self.muted_anim.set_target(if next_muted { 1.0 } else { 0.0 });
        changed
    }
}

impl BarWidget for VolumeWidget {
    fn id(&self) -> &'static str {
        "volume"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn icon(&self) -> Icon {
        Icon::Volume {
            level: self.level.position,
            muted: self.muted_anim.position,
        }
    }

    fn update(&mut self) -> bool {
        if !self.gate.should_run() {
            return false;
        }
        self.refresh()
    }

    fn on_click(&mut self) -> bool {
        // Click toggles mute via the same backend we read from.
        let new_muted = !self.muted;
        let ok = match self.backend {
            Backend::Wpctl => std::process::Command::new("wpctl")
                .args([
                    "set-mute",
                    "@DEFAULT_AUDIO_SINK@",
                    if new_muted { "1" } else { "0" },
                ])
                .status()
                .map(|s| s.success())
                .unwrap_or(false),
            Backend::Pactl => std::process::Command::new("pactl")
                .args([
                    "set-sink-mute",
                    "@DEFAULT_SINK@",
                    if new_muted { "1" } else { "0" },
                ])
                .status()
                .map(|s| s.success())
                .unwrap_or(false),
        };
        if !ok {
            // Even if the system call fails, animate the visual to give
            // immediate feedback; next poll will reconcile reality.
            log::warn!("volume mute toggle did not complete");
        }
        self.muted = new_muted;
        self.muted_anim
            .set_target(if new_muted { 1.0 } else { 0.0 });
        true
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        let mut alive = false;
        for s in [&mut self.level, &mut self.muted_anim] {
            if !s.at_rest() {
                s.step(dt);
                if !s.at_rest() {
                    alive = true;
                }
            }
        }
        alive
    }
}

struct VolReading {
    volume: u8,
    muted: bool,
}

fn detect_backend() -> Option<Backend> {
    if Command::new("wpctl").arg("--version").output().is_ok() {
        return Some(Backend::Wpctl);
    }
    if Command::new("pactl").arg("--version").output().is_ok() {
        return Some(Backend::Pactl);
    }
    None
}

/// `wpctl get-volume @DEFAULT_AUDIO_SINK@` →
/// "Volume: 0.42" or "Volume: 0.42 [MUTED]"
fn read_wpctl() -> Option<VolReading> {
    let out = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()?;
    let s = String::from_utf8(out.stdout).ok()?;
    let muted = s.contains("MUTED");
    let value: f32 = s
        .split_whitespace()
        .find_map(|tok| tok.parse::<f32>().ok())?;
    Some(VolReading {
        volume: (value * 100.0).round().clamp(0.0, 100.0) as u8,
        muted,
    })
}

/// `pactl get-sink-volume @DEFAULT_SINK@` → "Volume: front-left: 32768 / 50%/...".
/// `pactl get-sink-mute @DEFAULT_SINK@`   → "Mute: yes" / "Mute: no".
fn read_pactl() -> Option<VolReading> {
    let vol_out = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
        .ok()?;
    let mute_out = Command::new("pactl")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .output()
        .ok()?;
    let vol_str = String::from_utf8(vol_out.stdout).ok()?;
    let mute_str = String::from_utf8(mute_out.stdout).ok()?;

    let volume = vol_str
        .split('/')
        .map(str::trim)
        .find_map(|tok| {
            let pct = tok.strip_suffix('%')?;
            pct.trim().parse::<u8>().ok()
        })?;
    let muted = mute_str.contains("yes");

    Some(VolReading { volume, muted })
}
