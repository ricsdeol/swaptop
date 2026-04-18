#[allow(dead_code)]
pub(crate) fn matches_pattern(name: &str, pattern: &str) -> bool {
    match pattern.find('*') {
        None => name == pattern,
        Some(i) => {
            let prefix = &pattern[..i];
            let suffix = &pattern[i + 1..];
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_name() {
        assert!(matches_pattern("swapfile", "swapfile"));
    }

    #[test]
    fn prefix_wildcard_matches() {
        assert!(matches_pattern("swapfile", "swap*"));
        assert!(matches_pattern("swapfile1", "swap*"));
        assert!(matches_pattern("swap", "swap*"));
        assert!(matches_pattern("swap.img", "swap*"));
    }

    #[test]
    fn suffix_wildcard_matches() {
        assert!(matches_pattern("data.swap", "*.swap"));
        assert!(matches_pattern("big.swap", "*.swap"));
        assert!(matches_pattern("myfile.img", "*.img"));
    }

    #[test]
    fn prefix_and_suffix_wildcard_matches() {
        assert!(matches_pattern("swapfile1.bak", "swap*.bak"));
    }

    #[test]
    fn rejects_non_matching_names() {
        assert!(!matches_pattern("readme.txt", "swap*"));
        assert!(!matches_pattern("data.txt", "*.swap"));
        assert!(!matches_pattern("swapfile", "swapfile2"));
    }

    #[test]
    fn empty_name_matches_lone_star() {
        assert!(matches_pattern("", "*"));
    }

    #[test]
    fn empty_name_rejects_non_star_pattern() {
        assert!(!matches_pattern("", "swap*"));
    }

    #[test]
    fn pattern_without_star_requires_exact_match() {
        assert!(matches_pattern("swap.img", "swap.img"));
        assert!(!matches_pattern("swap.img2", "swap.img"));
    }
}
