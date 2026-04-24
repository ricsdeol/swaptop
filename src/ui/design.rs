/// Vertical gap between the main chrome sections (tab bar ↕ content ↕ status bar).
/// Must be ≥ 2 so that INNER_GAP = OUTER_GAP / 2 ≥ 1.
pub const OUTER_GAP: u16 = 2;

/// Vertical gap between sub-sections inside any tab view.
/// Always half of OUTER_GAP so inner spacing is subordinate to outer spacing.
pub const INNER_GAP: u16 = OUTER_GAP / 2;

#[cfg(test)]
mod tests {
    use super::{INNER_GAP, OUTER_GAP};

    #[test]
    fn inner_gap_is_half_of_outer_gap() {
        assert_eq!(INNER_GAP, OUTER_GAP / 2);
    }

    #[test]
    fn outer_gap_is_at_least_two_so_inner_gap_is_nonzero() {
        // These are compile-time constants; verified with const assertions below.
        const _: () = assert!(OUTER_GAP >= 2, "OUTER_GAP must be ≥ 2 so INNER_GAP ≥ 1");
        const _: () = assert!(INNER_GAP >= 1, "INNER_GAP must be ≥ 1 for visible spacing");
    }
}
