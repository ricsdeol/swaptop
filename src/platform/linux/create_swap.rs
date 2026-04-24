use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command as StdCommand;

use tokio::sync::mpsc::UnboundedSender;

use crate::actions::Action;
use crate::create_swap::StepStatus;
use crate::platform::parse_swap_header;

// ── Pure helper functions ─────────────────────────────────────────────────────

/// Parse `/proc/mounts` content and return the filesystem type covering `target`.
///
/// Picks the longest mount-point prefix that is a parent of `target`.
pub fn detect_fs_type(mounts_content: &str, target: &std::path::Path) -> Option<String> {
    let target_str = target.to_string_lossy();
    let mut best: Option<(usize, String)> = None;
    for line in mounts_content.lines() {
        let mut parts = line.split_whitespace();
        let (Some(_dev), Some(mount_point), Some(fs_type)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if target_str.starts_with(mount_point)
            && (target_str.len() == mount_point.len()
                || target_str.as_bytes().get(mount_point.len()) == Some(&b'/')
                || mount_point == "/")
        {
            let len = mount_point.len();
            if best.as_ref().map(|(l, _)| len > *l).unwrap_or(true) {
                best = Some((len, fs_type.to_string()));
            }
        }
    }
    best.map(|(_, fs)| fs)
}

/// Given the filesystem type, decide whether to use `fallocate` or `dd`.
pub fn allocator_for_fs(fs_type: &str) -> Allocator {
    // btrfs uses `dd` because `fallocate` produces preallocated extents that the
    // kernel rejects for swap (swapon fails with EINVAL). The user must additionally
    // disable COW on the target file (`chattr +C`) before running swaptop; we do not
    // do that automatically because it only takes effect on empty files.
    match fs_type {
        "ext2" | "ext3" | "ext4" | "xfs" | "f2fs" => Allocator::Fallocate,
        _ => Allocator::Dd,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allocator {
    Fallocate,
    Dd,
}

impl Allocator {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fallocate => "fallocate",
            Self::Dd => "dd",
        }
    }
}

// ── Background step runner ────────────────────────────────────────────────────

/// Run all create-swap steps in `spawn_blocking`. Sends `CreateSwapStepUpdate`
/// for each step transition. On `activate_only`, skips to Step 6 (swapon).
#[allow(clippy::too_many_arguments)]
pub fn run_create_swap_steps(
    path: PathBuf,
    size_bytes: u64,
    priority: i16,
    activate_after: bool,
    activate_only: bool,
    tx: UnboundedSender<Action>,
) {
    let send = |idx: usize, status: StepStatus| {
        let _ = tx.send(Action::CreateSwapStepUpdate { index: idx, status });
    };

    // activate_only path: go straight to Step 6 (swapon).
    if activate_only {
        send(6, StepStatus::Running);
        match do_swapon_with_priority(&path, priority) {
            Ok(()) => send(6, StepStatus::Done),
            Err(e) => send(6, StepStatus::Error(e)),
        }
        return;
    }

    // Step 0 — Disk space
    send(0, StepStatus::Running);
    let parent = path.parent().unwrap_or(std::path::Path::new("/"));
    match check_disk_space(parent, size_bytes) {
        Ok(()) => send(0, StepStatus::Done),
        Err(e) => {
            send(0, StepStatus::Error(e));
            return;
        }
    }

    // Step 1 — File existence / magic
    send(1, StepStatus::Running);
    match check_target_file(&path) {
        TargetFileCheck::DoesNotExist => send(1, StepStatus::Done),
        TargetFileCheck::AlreadySwap { size } => {
            send(1, StepStatus::Done);
            let _ = tx.send(Action::OpenConfirmActivateOnly {
                path: path.clone(),
                size_bytes: size,
            });
            return;
        }
        TargetFileCheck::ExistsNotSwap => {
            send(
                1,
                StepStatus::Error(
                    "file exists and is not a swap file — refusing to overwrite".to_string(),
                ),
            );
            return;
        }
        TargetFileCheck::IoError(e) => {
            send(1, StepStatus::Error(format!("cannot inspect target: {e}")));
            return;
        }
    }

    // Step 2 — Filesystem detection
    send(2, StepStatus::Running);
    let fs_type = match fs::read_to_string("/proc/mounts") {
        Ok(content) => detect_fs_type(&content, &path).unwrap_or_else(|| "unknown".to_string()),
        Err(_) => "unknown".to_string(),
    };
    let allocator = allocator_for_fs(&fs_type);
    send(2, StepStatus::Done);

    // Step 3 — Allocate
    send(3, StepStatus::Running);
    let alloc_result = match allocator {
        Allocator::Fallocate => run_cmd(
            StdCommand::new("fallocate")
                .arg("-l")
                .arg(size_bytes.to_string())
                .arg(&path),
        ),
        Allocator::Dd => {
            let mb = size_bytes / (1024 * 1024);
            let mb = if mb == 0 { 1 } else { mb };
            run_cmd(
                StdCommand::new("dd")
                    .arg("if=/dev/zero")
                    .arg(format!("of={}", path.display()))
                    .arg("bs=1M")
                    .arg(format!("count={mb}")),
            )
        }
    };
    match alloc_result {
        Ok(()) => send(3, StepStatus::Done),
        Err(e) => {
            send(
                3,
                StepStatus::Error(format!("{} failed: {e}", allocator.label())),
            );
            return;
        }
    }

    // Step 4 — chmod 600
    send(4, StepStatus::Running);
    match fs::set_permissions(&path, fs::Permissions::from_mode(0o600)) {
        Ok(()) => send(4, StepStatus::Done),
        Err(e) => {
            send(4, StepStatus::Error(format!("chmod failed: {e}")));
            return;
        }
    }

    // Step 5 — mkswap
    send(5, StepStatus::Running);
    match run_cmd(StdCommand::new("mkswap").arg(&path)) {
        Ok(()) => send(5, StepStatus::Done),
        Err(e) => {
            send(5, StepStatus::Error(format!("mkswap failed: {e}")));
            return;
        }
    }

    // Step 6 — swapon (skipped if activate_after is false)
    if activate_after {
        send(6, StepStatus::Running);
        match do_swapon_with_priority(&path, priority) {
            Ok(()) => send(6, StepStatus::Done),
            Err(e) => {
                send(6, StepStatus::Error(e));
            }
        }
    } else {
        send(6, StepStatus::Done);
    }
}

enum TargetFileCheck {
    DoesNotExist,
    AlreadySwap { size: u64 },
    ExistsNotSwap,
    IoError(String),
}

fn check_target_file(path: &std::path::Path) -> TargetFileCheck {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return TargetFileCheck::DoesNotExist,
        Err(e) => return TargetFileCheck::IoError(e.to_string()),
    };
    if !meta.is_file() {
        return TargetFileCheck::ExistsNotSwap;
    }
    let size = meta.len();
    let mut f = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return TargetFileCheck::IoError(e.to_string()),
    };
    let mut buf = vec![0u8; 4096];
    match f.read_exact(&mut buf) {
        Ok(()) => {
            if let Some(size) = parse_swap_header(&buf, size) {
                TargetFileCheck::AlreadySwap { size }
            } else {
                TargetFileCheck::ExistsNotSwap
            }
        }
        Err(_) => TargetFileCheck::ExistsNotSwap,
    }
}

fn check_disk_space(parent: &std::path::Path, needed: u64) -> Result<(), String> {
    let stat = nix::sys::statvfs::statvfs(parent).map_err(|e| e.to_string())?;
    let available = stat.blocks_available() as u64 * stat.fragment_size() as u64;
    let required = needed + needed / 10;
    if available >= required {
        Ok(())
    } else {
        Err(format!(
            "not enough space: need {} (incl. 10% margin), have {}",
            human_bytes::human_bytes(required as f64),
            human_bytes::human_bytes(available as f64),
        ))
    }
}

fn run_cmd(cmd: &mut StdCommand) -> Result<(), String> {
    let output = cmd.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_string())
    }
}

fn do_swapon_with_priority(path: &std::path::Path, priority: i16) -> Result<(), String> {
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    // SYS_swapon flags: SWAP_FLAG_PREFER(=0x8000) | (priority & SWAP_FLAG_PRIO_MASK=0x7fff)
    // If priority == -1, omit the prefer flag (kernel-default priority).
    let flags: i32 = if priority < 0 {
        0
    } else {
        0x8000 | (priority as i32 & 0x7fff)
    };
    // SAFETY: `c` is a valid NUL-terminated C string pointing to a valid path.
    let ret = unsafe { nix::libc::swapon(c.as_ptr(), flags) };
    if ret == 0 {
        Ok(())
    } else {
        Err(format!(
            "swapon failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_fs_type_matches_root_mount() {
        let mounts = "\
/dev/sda1 / ext4 rw 0 0
proc /proc proc rw 0 0
tmpfs /tmp tmpfs rw 0 0
";
        let fs = detect_fs_type(mounts, std::path::Path::new("/swapfile"));
        assert_eq!(fs.as_deref(), Some("ext4"));
    }

    #[test]
    fn detect_fs_type_prefers_longest_match() {
        let mounts = "\
/dev/sda1 / ext4 rw 0 0
tmpfs /home/user/ramdisk tmpfs rw 0 0
";
        let fs = detect_fs_type(mounts, std::path::Path::new("/home/user/ramdisk/swapfile"));
        assert_eq!(fs.as_deref(), Some("tmpfs"));
    }

    #[test]
    fn detect_fs_type_ignores_unrelated_mount_point() {
        let mounts = "/dev/sda1 / ext4 rw 0 0\ntmpfs /tmp tmpfs rw 0 0\n";
        let fs = detect_fs_type(mounts, std::path::Path::new("/var/swapfile"));
        assert_eq!(fs.as_deref(), Some("ext4"));
    }

    #[test]
    fn allocator_for_fs_picks_fallocate_on_ext4() {
        assert_eq!(allocator_for_fs("ext4"), Allocator::Fallocate);
        assert_eq!(allocator_for_fs("xfs"), Allocator::Fallocate);
    }

    #[test]
    fn allocator_for_fs_falls_back_to_dd_on_tmpfs_or_unknown() {
        assert_eq!(allocator_for_fs("tmpfs"), Allocator::Dd);
        assert_eq!(allocator_for_fs("ramfs"), Allocator::Dd);
        assert_eq!(allocator_for_fs("whatever"), Allocator::Dd);
    }

    #[test]
    fn allocator_for_fs_uses_dd_on_btrfs() {
        // fallocate on btrfs creates prealloc extents that swapon rejects.
        assert_eq!(allocator_for_fs("btrfs"), Allocator::Dd);
    }
}
