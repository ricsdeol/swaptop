/// Fields extracted from `/proc/{pid}/status`.
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
struct StatusInfo {
    name: String,
    uid:  u32,
    rss:  u64,  // bytes
    vms:  u64,  // bytes
    swap: u64,  // bytes
}

/// Parse `/proc/{pid}/status` content into a `StatusInfo`.
/// Returns `None` if required fields are missing.
#[allow(dead_code)]
fn parse_status(content: &str) -> Option<StatusInfo> {
    let mut name: Option<String> = None;
    let mut uid: Option<u32> = None;
    let mut rss: u64 = 0;
    let mut vms: u64 = 0;
    let mut swap: u64 = 0;

    for line in content.lines() {
        if let Some(v) = line.strip_prefix("Name:\t") {
            name = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("Uid:\t") {
            uid = v.split_whitespace().next()?.parse().ok();
        } else if let Some(v) = line.strip_prefix("VmRSS:") {
            rss = parse_kb_value(v);
        } else if let Some(v) = line.strip_prefix("VmSize:") {
            vms = parse_kb_value(v);
        } else if let Some(v) = line.strip_prefix("VmSwap:") {
            swap = parse_kb_value(v);
        }
    }

    Some(StatusInfo {
        name: name?,
        uid: uid?,
        rss,
        vms,
        swap,
    })
}

/// Parse a value like `"  524288 kB"` into bytes.
#[allow(dead_code)]
fn parse_kb_value(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024
}

/// Parse `/proc/{pid}/stat` and return `utime + stime` (total CPU ticks).
/// Returns `None` if the format is unexpected.
#[allow(dead_code)]
fn parse_stat_cpu_ticks(content: &str) -> Option<u64> {
    // Find the last ')' to skip the comm field (which can contain spaces and parens).
    let after_comm = content.rfind(')')? + 1;
    let fields: Vec<&str> = content[after_comm..].split_whitespace().collect();
    // After ')': state(0) ppid(1) pgrp(2) session(3) tty_nr(4) tpgid(5) flags(6)
    //            minflt(7) cminflt(8) majflt(9) cmajflt(10) utime(11) stime(12)
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_extracts_all_fields() {
        let content = "\
Name:\tfirefox
Umask:\t0022
State:\tS (sleeping)
Tgid:\t1234
Pid:\t1234
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t 3145728 kB
VmSize:\t 2097152 kB
VmRSS:\t  524288 kB
VmSwap:\t  131072 kB
Threads:\t4
";
        let info = parse_status(content).unwrap();
        assert_eq!(info.name, "firefox");
        assert_eq!(info.uid, 1000);
        assert_eq!(info.rss, 524288 * 1024);
        assert_eq!(info.vms, 2097152 * 1024);
        assert_eq!(info.swap, 131072 * 1024);
    }

    #[test]
    fn parse_status_returns_none_when_name_missing() {
        let content = "\
Uid:\t1000\t1000\t1000\t1000
VmSize:\t 2097152 kB
VmRSS:\t  524288 kB
VmSwap:\t  131072 kB
";
        assert!(parse_status(content).is_none());
    }

    #[test]
    fn parse_status_handles_kernel_thread_without_vm_fields() {
        let content = "\
Name:\t[kworker/0:0]
State:\tI (idle)
Pid:\t42
Uid:\t0\t0\t0\t0
Gid:\t0\t0\t0\t0
Threads:\t1
";
        let info = parse_status(content).unwrap();
        assert_eq!(info.name, "[kworker/0:0]");
        assert_eq!(info.uid, 0);
        assert_eq!(info.rss, 0);
        assert_eq!(info.vms, 0);
        assert_eq!(info.swap, 0);
    }

    #[test]
    fn parse_status_handles_zero_swap() {
        let content = "\
Name:\tbash
Uid:\t1000\t1000\t1000\t1000
VmSize:\t 102400 kB
VmRSS:\t   51200 kB
VmSwap:\t       0 kB
";
        let info = parse_status(content).unwrap();
        assert_eq!(info.swap, 0);
    }

    #[test]
    fn parse_stat_extracts_utime_plus_stime() {
        // Fields: pid (comm) state ppid pgrp session tty_nr tpgid flags
        //         minflt cminflt majflt cmajflt utime stime ...
        let content = "1234 (firefox) S 1000 1234 1234 0 -1 4194304 \
                       1000 0 100 0 54321 12345 0 0 20 0 4 0 1000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let ticks = parse_stat_cpu_ticks(content).unwrap();
        assert_eq!(ticks, 54321 + 12345);
    }

    #[test]
    fn parse_stat_handles_comm_with_spaces() {
        let content = "5678 (Web Content) S 1000 5678 5678 0 -1 4194304 \
                       500 0 50 0 11111 22222 0 0 20 0 1 0 2000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let ticks = parse_stat_cpu_ticks(content).unwrap();
        assert_eq!(ticks, 11111 + 22222);
    }

    #[test]
    fn parse_stat_handles_comm_with_parentheses() {
        let content = "9999 (my (app) name) S 1000 9999 9999 0 -1 4194304 \
                       200 0 10 0 100 200 0 0 20 0 1 0 3000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let ticks = parse_stat_cpu_ticks(content).unwrap();
        assert_eq!(ticks, 100 + 200);
    }

    #[test]
    fn parse_stat_returns_none_for_garbage() {
        assert!(parse_stat_cpu_ticks("not a stat line").is_none());
    }
}
