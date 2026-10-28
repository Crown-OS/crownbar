use std::borrow::Cow;

/// `text` cut to at most `max` characters, the last of them an ellipsis.
pub fn ellipsize(text: &str, max: usize) -> Cow<'_, str> {
    match text.char_indices().nth(max.saturating_sub(1)) {
        Some((end, _)) if text[end..].chars().nth(1).is_some() => {
            Cow::Owned(format!("{}…", text[..end].trim_end()))
        }
        _ => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_long_text_is_cut() {
        assert_eq!(ellipsize("bar.rs", 8), "bar.rs");
        assert_eq!(ellipsize("12345678", 8), "12345678");
        assert_eq!(ellipsize("123456789", 8), "1234567…");
        assert_eq!(ellipsize("ab  cdefghij", 5), "ab…");
    }
}
