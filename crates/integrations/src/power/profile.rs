//! The three profiles power-profiles-daemon defines.

/// A machine advertises a subset of these — a desktop with no
/// `platform_profile` driver has no `power-saver` — so a panel is always built
/// from [`Profiles::supported`], never from this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Saver,
    Balanced,
    Performance,
}

impl Profile {
    pub fn id(self) -> &'static str {
        match self {
            Self::Saver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    /// What the panel calls it, which is what the rest of the desktop calls it
    /// rather than what the daemon does.
    pub fn label(self) -> &'static str {
        match self {
            Self::Saver => "Low Power",
            Self::Balanced => "Balanced",
            Self::Performance => "High Power",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "power-saver" => Some(Self::Saver),
            "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }
}

/// What the daemon offers, what it is on, and whether the hardware is holding
/// it back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profiles {
    pub supported: Vec<Profile>,
    pub active: Option<Profile>,
    /// The daemon's `PerformanceDegraded`: non-empty means thermally or
    /// lap-limited, which the panel says out loud rather than leaving the user
    /// wondering why High Power feels like Balanced.
    pub degraded: Option<String>,
}

impl Profiles {
    pub fn is_saving(&self) -> bool {
        self.active == Some(Profile::Saver)
    }
}
