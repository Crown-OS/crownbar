//! One reading of the cell, and the words the bar puts under it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargeStatus {
    Charging,
    Discharging,
    Full,
    Empty,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Charge {
    /// ∈ [0, 1].
    pub level: f32,
    pub status: ChargeStatus,
    /// Time to full while charging, to empty otherwise.
    pub minutes: Option<u32>,
    /// Full charge against design capacity, ∈ [0, 1].
    pub health: Option<f32>,
}

impl Charge {
    /// What a machine with no reading shows: a flat cell, so the springs have
    /// somewhere to sit before the first poll lands.
    pub const EMPTY: Self = Self {
        level: 0.0,
        status: ChargeStatus::Unknown,
        minutes: None,
        health: None,
    };

    pub fn percent(self) -> u8 {
        (self.level * 100.0).round().clamp(0.0, 100.0) as u8
    }

    pub fn charging(self) -> bool {
        matches!(self.status, ChargeStatus::Charging)
    }

    pub fn full(self) -> bool {
        matches!(self.status, ChargeStatus::Full)
    }

    /// The line under the reading. Worded here rather than in the widget so
    /// the bar, a future OSD and crownsettings cannot disagree.
    pub fn summary(self) -> String {
        match (self.charging(), self.full(), self.minutes) {
            (_, true, _) => "Charged".into(),
            (true, _, Some(m)) => format!("{} until full", clock(m)),
            (true, _, None) => "Charging".into(),
            (false, _, Some(m)) => format!("{} remaining", clock(m)),
            (false, _, None) => "On Battery".into(),
        }
    }
}

/// Minutes as `h:mm`, or as plain minutes under the hour.
fn clock(minutes: u32) -> String {
    match minutes / 60 {
        0 => format!("{minutes} min"),
        hours => format!("{hours}:{:02}", minutes % 60),
    }
}
