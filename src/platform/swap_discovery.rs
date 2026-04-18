use std::path::Path;

use crate::create_swap::detect_swap_magic;
use crate::platform::{SwapDevice, SwapKind};

/// Matches `name` against `pattern` containing at most one `*` wildcard.
// Temporary: remove allow once discover_inactive_swap_files is added (Task 3)
#[allow(dead_code)]
fn matches_pattern(name: &str, pattern: &str) -> bool {
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

/// Check if `path` is a regular file with a valid swap magic header.
/// Returns `None` silently on any I/O or permission error.
pub(crate) fn probe_swap_file(path: &Path) -> Option<SwapDevice> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let size = meta.len();
    if size < 4096 {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    std::io::Read::read_exact(&mut f, &mut buf).ok()?;
    detect_swap_magic(&buf, size)?;
    Some(SwapDevice {
        path: path.to_path_buf(),
        total: size,
        used: 0,
        priority: 0,
        kind: SwapKind::File,
        active: false,
    })
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

    #[test]
    fn probe_returns_none_for_nonexistent() {
        let result = probe_swap_file(Path::new("/tmp/nonexistent_swap_probe_test_xyz"));
        assert!(result.is_none());
    }

    #[test]
    fn probe_returns_none_for_small_file() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("swaptop_discovery_test_small");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0u8; 512]).unwrap();
        drop(f);
        assert!(probe_swap_file(&path).is_none());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn probe_returns_none_for_non_swap() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("swaptop_discovery_test_non_swap");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0u8; 4096]).unwrap();
        drop(f);

        let result = probe_swap_file(&path);
        assert!(result.is_none());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn probe_returns_device_for_swap_magic() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("swaptop_discovery_test_swap_magic");
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&buf).unwrap();
        drop(f);

        let result = probe_swap_file(&path);
        assert!(result.is_some());
        let dev = result.unwrap();
        assert_eq!(dev.path, path);
        assert!(!dev.active);
        assert!(matches!(dev.kind, SwapKind::File));
        assert_eq!(dev.total, 4096);

        std::fs::remove_file(&path).unwrap();
    }
}
