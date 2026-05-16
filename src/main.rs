use std::collections::HashSet;
use std::env;
use std::process;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System};

const BYTES_PER_GB: u64 = 1024 * 1024 * 1024;
const DEFAULT_THRESHOLD_GB: u64 = 50;
const DEFAULT_INTERVAL_SECS: u64 = 5;

fn protected_names() -> HashSet<&'static str> {
    [
        "launchd",
        "kernel_task",
        "WindowServer",
        "loginwindow",
        "Finder",
        "Dock",
        "SystemUIServer",
        "ControlCenter",
        "NotificationCenter",
        "coreaudiod",
        "powerd",
        "configd",
        "logd",
        "opendirectoryd",
        "securityd",
        "trustd",
        "mds",
        "mds_stores",
        "cfprefsd",
        "distnoted",
        "UserEventAgent",
        "syspolicyd",
        "tccd",
        "sandboxd",
        "WindowManager",
        "sshd",
        "sshd-session",
        "ram-monitor",
    ]
    .into_iter()
    .collect()
}

fn log(msg: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("[{}] {}", ts, msg);
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let threshold_gb = env_u64("RAM_THRESHOLD_GB", DEFAULT_THRESHOLD_GB);
    let interval_secs = env_u64("RAM_INTERVAL_SECS", DEFAULT_INTERVAL_SECS);
    let dry_run = env::var("RAM_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let threshold_bytes = threshold_gb.saturating_mul(BYTES_PER_GB);
    let self_pid = process::id();
    let protected = protected_names();

    log(&format!(
        "ram-monitor starting: threshold={}GB interval={}s dry_run={} self_pid={}",
        threshold_gb, interval_secs, dry_run, self_pid
    ));

    let mut sys = System::new();

    loop {
        sys.refresh_memory();
        let used = sys.used_memory();
        let total = sys.total_memory();
        let used_gb = used as f64 / BYTES_PER_GB as f64;
        let total_gb = total as f64 / BYTES_PER_GB as f64;

        if used >= threshold_bytes {
            log(&format!(
                "OVER THRESHOLD: used={:.2}GB total={:.2}GB threshold={}GB - culling",
                used_gb, total_gb, threshold_gb
            ));

            sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::new().with_memory(),
            );

            let mut procs: Vec<(Pid, String, u64)> = sys
                .processes()
                .iter()
                .filter(|(pid, p)| {
                    let pid_u32 = pid.as_u32();
                    if pid_u32 == self_pid {
                        return false;
                    }
                    if pid_u32 <= 1 {
                        return false;
                    }
                    let name = p.name().to_string_lossy();
                    if protected.contains(name.as_ref()) {
                        return false;
                    }
                    true
                })
                .map(|(pid, p)| (*pid, p.name().to_string_lossy().into_owned(), p.memory()))
                .collect();

            procs.sort_by(|a, b| b.2.cmp(&a.2));

            let mut current_used = used;
            for (pid, name, mem) in procs.iter() {
                if current_used < threshold_bytes {
                    break;
                }
                let mem_gb = *mem as f64 / BYTES_PER_GB as f64;
                if dry_run {
                    log(&format!(
                        "DRY_RUN would kill pid={} name={} rss={:.2}GB",
                        pid.as_u32(),
                        name,
                        mem_gb
                    ));
                } else if let Some(p) = sys.process(*pid) {
                    let sent = p.kill_with(Signal::Term).unwrap_or(false);
                    log(&format!(
                        "killed pid={} name={} rss={:.2}GB signal=TERM ok={}",
                        pid.as_u32(),
                        name,
                        mem_gb,
                        sent
                    ));
                }
                current_used = current_used.saturating_sub(*mem);
            }

            thread::sleep(Duration::from_secs(2));
            sys.refresh_memory();
            let new_used_gb = sys.used_memory() as f64 / BYTES_PER_GB as f64;
            log(&format!(
                "post-cull used={:.2}GB threshold={}GB",
                new_used_gb, threshold_gb
            ));
        } else {
            log(&format!(
                "ok: used={:.2}GB / total={:.2}GB (threshold={}GB)",
                used_gb, total_gb, threshold_gb
            ));
        }

        thread::sleep(Duration::from_secs(interval_secs));
    }
}
