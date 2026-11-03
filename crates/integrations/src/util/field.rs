/// Store `value` in `slot`, reporting whether that changed anything — the
/// answer a snapshot edit returns to decide whether the bar is woken.
pub fn set_changed<T: PartialEq>(slot: &mut T, value: T) -> bool {
    let changed = *slot != value;
    if changed {
        *slot = value;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_new_value_is_a_change() {
        let mut held = 3;
        assert!(!set_changed(&mut held, 3));
        assert!(set_changed(&mut held, 4));
        assert_eq!(held, 4);
    }
}
