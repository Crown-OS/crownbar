use std::time::Duration;

/// `90`, `90s`, `25m`, `1h30m`.
pub fn parse(text: &str) -> Result<Duration, String> {
    let mut total = 0u64;
    let mut digits = 0u64;
    let mut pending = false;
    for c in text.trim().chars() {
        if let Some(digit) = c.to_digit(10) {
            digits = digits * 10 + u64::from(digit);
            pending = true;
            continue;
        }
        let unit = match c {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return Err(format!("`{text}` is no duration: unexpected `{c}`")),
        };
        if !pending {
            return Err(format!("`{text}` is no duration: `{c}` has no number"));
        }
        total += digits * unit;
        (digits, pending) = (0, false);
    }
    total += digits;
    match total {
        0 => Err(format!("`{text}` is no duration")),
        seconds => Ok(Duration::from_secs(seconds)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_add_up() {
        assert_eq!(parse("90"), Ok(Duration::from_secs(90)));
        assert_eq!(parse("25m"), Ok(Duration::from_secs(1500)));
        assert_eq!(parse("1h30m"), Ok(Duration::from_secs(5400)));
        assert_eq!(parse("2m10s"), Ok(Duration::from_secs(130)));
        assert!(parse("m").is_err());
        assert!(parse("5x").is_err());
        assert!(parse("0").is_err());
    }
}
