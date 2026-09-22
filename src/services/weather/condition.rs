//! What the sky is doing, and how a WMO code maps onto it.

/// Deliberately coarser than the code it is mapped from: a pill has one
/// glyph's worth of room, and the difference between light and moderate
/// drizzle is not something it can show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Condition {
    #[default]
    Clear,
    PartlyCloudy,
    Cloudy,
    Overcast,
    Drizzle,
    Rain,
    /// Passing rain with the sun still out.
    Showers,
    Thunder,
    Snow,
    /// Rain and snow together, or freezing rain.
    Sleet,
    Fog,
    /// Dust, smoke and haze — a moon behind bars, day or night.
    Haze,
    Wind,
}

impl Condition {
    /// WMO 4677 present-weather code, as Open-Meteo reports it.
    ///
    /// The codes are a fine-grained meteorological vocabulary and this is a
    /// bar, so whole families collapse: every intensity of drizzle is drizzle,
    /// and freezing rain joins sleet because they are the same warning.
    pub fn from_wmo(code: u8) -> Self {
        match code {
            0 => Self::Clear,
            1 | 2 => Self::PartlyCloudy,
            3 => Self::Overcast,
            45 | 48 => Self::Fog,
            51 | 53 | 55 => Self::Drizzle,
            56 | 57 | 66 | 67 => Self::Sleet,
            61 | 63 | 65 => Self::Rain,
            71 | 73 | 75 | 77 | 85 | 86 => Self::Snow,
            80 | 81 | 82 => Self::Showers,
            95 | 96 | 99 => Self::Thunder,
            // Not a code Open-Meteo issues, but the field is a `u8` and a
            // sky the bar cannot name should read as overcast rather than as
            // a clear day.
            _ => Self::Cloudy,
        }
    }

    /// What the panel calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => "Clear",
            Self::PartlyCloudy => "Partly Cloudy",
            Self::Cloudy => "Cloudy",
            Self::Overcast => "Overcast",
            Self::Drizzle => "Drizzle",
            Self::Rain => "Rain",
            Self::Showers => "Showers",
            Self::Thunder => "Thunderstorms",
            Self::Snow => "Snow",
            Self::Sleet => "Sleet",
            Self::Fog => "Fog",
            Self::Haze => "Haze",
            Self::Wind => "Windy",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codes_that_matter_map_to_the_glyph_that_shows_them() {
        for (code, expected) in [
            (0u8, Condition::Clear),
            (2, Condition::PartlyCloudy),
            (3, Condition::Overcast),
            (48, Condition::Fog),
            (55, Condition::Drizzle),
            (65, Condition::Rain),
            (67, Condition::Sleet),
            (75, Condition::Snow),
            (81, Condition::Showers),
            (99, Condition::Thunder),
        ] {
            assert_eq!(Condition::from_wmo(code), expected, "WMO {code}");
        }
    }

    #[test]
    fn an_unknown_code_is_never_reported_as_a_clear_sky() {
        for code in [4u8, 30, 100, 255] {
            assert_ne!(Condition::from_wmo(code), Condition::Clear, "WMO {code}");
        }
    }
}
