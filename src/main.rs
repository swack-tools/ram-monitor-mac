use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System};

const BYTES_PER_GB: u64 = 1024 * 1024 * 1024;
const DEFAULT_THRESHOLD_GB: u64 = 50;
const DEFAULT_INTERVAL_SECS: u64 = 5;

const LABEL: &str = "com.swack.ram-monitor";
const PLIST_PATH: &str = "/Library/LaunchDaemons/com.swack.ram-monitor.plist";
const APP_BIN_PATH: &str = "/Applications/ram-monitor.app/Contents/MacOS/ram-monitor";
const LOG_DIR: &str = "/Library/Logs/ram-monitor";

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

fn is_root() -> bool {
    libc_geteuid() == 0
}

extern "C" {
    fn geteuid() -> u32;
    fn getppid() -> i32;
    fn isatty(fd: i32) -> i32;
}

fn libc_geteuid() -> u32 {
    unsafe { geteuid() }
}

fn launched_by_launchd() -> bool {
    unsafe { getppid() == 1 }
}

fn stdin_is_tty() -> bool {
    unsafe { isatty(0) == 1 }
}

fn self_exe() -> PathBuf {
    env::current_exe().unwrap_or_else(|_| PathBuf::from(APP_BIN_PATH))
}

// -------- monitor mode -----------------------------------------------------

fn run_monitor() -> ! {
    let threshold_gb = env_u64("RAM_THRESHOLD_GB", DEFAULT_THRESHOLD_GB);
    let interval_secs = env_u64("RAM_INTERVAL_SECS", DEFAULT_INTERVAL_SECS);
    let dry_run = env::var("RAM_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let threshold_bytes = threshold_gb.saturating_mul(BYTES_PER_GB);
    let self_pid = process::id();
    let protected = protected_names();

    log(&format!(
        "ram-monitor starting: threshold={}GB interval={}s dry_run={} self_pid={} euid={}",
        threshold_gb, interval_secs, dry_run, self_pid, libc_geteuid()
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

// -------- install / uninstall ---------------------------------------------

const PLIST_BODY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.swack.ram-monitor</string>

    <key>ProgramArguments</key>
    <array>
        <string>/Applications/ram-monitor.app/Contents/MacOS/ram-monitor</string>
        <string>monitor</string>
    </array>

    <key>EnvironmentVariables</key>
    <dict>
        <key>RAM_THRESHOLD_GB</key>
        <string>50</string>
        <key>RAM_INTERVAL_SECS</key>
        <string>5</string>
        <key>RAM_DRY_RUN</key>
        <string>0</string>
    </dict>

    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>Nice</key>
    <integer>10</integer>
    <key>StandardOutPath</key>
    <string>/Library/Logs/ram-monitor/ram-monitor.log</string>
    <key>StandardErrorPath</key>
    <string>/Library/Logs/ram-monitor/ram-monitor.err.log</string>
</dict>
</plist>
"#;

/// Escape a string for embedding in an AppleScript string literal.
fn escape_for_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Re-exec the current binary via osascript with administrator privileges,
/// passing along the subcommand. This produces a native GUI password prompt
/// when the user double-clicks the .app from Finder.
fn elevate_with_osascript(subcommand: &str) -> ! {
    let exe = self_exe();
    let exe_str = exe.to_string_lossy();
    let shell_cmd = format!("{:?} {}", exe_str, subcommand);
    let osa = format!(
        "do shell script \"{}\" with administrator privileges",
        escape_for_applescript(&shell_cmd)
    );
    let status = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&osa)
        .status();
    match status {
        Ok(s) if s.success() => process::exit(0),
        Ok(s) => {
            eprintln!("osascript exited with {:?}", s.code());
            process::exit(s.code().unwrap_or(1));
        }
        Err(e) => {
            eprintln!("failed to launch osascript: {e}");
            process::exit(1);
        }
    }
}

fn ensure_root_or_elevate(subcommand: &str) {
    if is_root() {
        return;
    }
    if stdin_is_tty() {
        // run under sudo so the password prompt shows in this terminal
        let exe = self_exe();
        let err = Command::new("/usr/bin/sudo")
            .arg("-E")
            .arg(exe)
            .arg(subcommand)
            .exec();
        eprintln!("failed to exec sudo: {err}");
        process::exit(1);
    }
    elevate_with_osascript(subcommand);
}

fn run_install() -> ! {
    ensure_root_or_elevate("install");

    println!("==> Installing {LABEL}");

    // unload any existing version
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("system/{LABEL}")])
        .status();

    // write plist
    if let Err(e) = fs::write(PLIST_PATH, PLIST_BODY) {
        eprintln!("failed to write {PLIST_PATH}: {e}");
        process::exit(1);
    }
    let _ = Command::new("/bin/chmod").args(["644", PLIST_PATH]).status();
    let _ = Command::new("/usr/sbin/chown")
        .args(["root:wheel", PLIST_PATH])
        .status();

    // log dir
    let _ = fs::create_dir_all(LOG_DIR);
    let _ = Command::new("/usr/sbin/chown")
        .args(["root:wheel", LOG_DIR])
        .status();
    let _ = Command::new("/bin/chmod").args(["755", LOG_DIR]).status();

    // lint
    let lint = Command::new("/usr/bin/plutil")
        .args(["-lint", PLIST_PATH])
        .status();
    if !lint.map(|s| s.success()).unwrap_or(false) {
        eprintln!("plutil rejected {PLIST_PATH}");
        process::exit(1);
    }

    // bootstrap
    let bs = Command::new("/bin/launchctl")
        .args(["bootstrap", "system", PLIST_PATH])
        .status();
    if !bs.map(|s| s.success()).unwrap_or(false) {
        eprintln!("launchctl bootstrap failed");
        process::exit(1);
    }
    let _ = Command::new("/bin/launchctl")
        .args(["enable", &format!("system/{LABEL}")])
        .status();
    let _ = Command::new("/bin/launchctl")
        .args(["kickstart", "-k", &format!("system/{LABEL}")])
        .status();

    println!("==> Installed at {APP_BIN_PATH}");
    println!("==> Plist:        {PLIST_PATH}");
    println!("==> Logs:         {LOG_DIR}/ram-monitor.log");
    process::exit(0);
}

fn run_uninstall() -> ! {
    ensure_root_or_elevate("uninstall");

    println!("==> Uninstalling {LABEL}");
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("system/{LABEL}")])
        .status();
    let _ = fs::remove_file(PLIST_PATH);
    if Path::new("/Applications/ram-monitor.app").exists() {
        let _ = fs::remove_dir_all("/Applications/ram-monitor.app");
        println!("  removed /Applications/ram-monitor.app");
    }
    println!("==> Done. Logs in {LOG_DIR}/ left in place.");
    process::exit(0);
}

fn print_usage() {
    eprintln!(
        "ram-monitor — kills highest-RAM processes when used memory exceeds a threshold.\n\n\
         Usage: ram-monitor <command>\n\n\
         Commands:\n  \
           monitor     run the monitor loop (used by launchd)\n  \
           install     install as a system LaunchDaemon (requires root)\n  \
           uninstall   remove the system LaunchDaemon (requires root)\n"
    );
}

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let first = args.first().and_then(|s| s.to_str()).map(str::to_string);

    match first.as_deref() {
        Some("monitor") => run_monitor(),
        Some("install") => run_install(),
        Some("uninstall") => run_uninstall(),
        Some("--help") | Some("-h") | Some("help") => {
            print_usage();
        }
        None => {
            // No args: distinguish daemon (launched by launchd) from user
            // double-click. launchd ALWAYS passes a "monitor" arg via our
            // plist, so a no-arg launch must be Finder/Terminal.
            if launched_by_launchd() {
                run_monitor();
            } else {
                // First-run install flow.
                eprintln!("ram-monitor not yet installed — installing now.");
                run_install();
            }
        }
        Some(other) => {
            eprintln!("unknown command: {other}");
            print_usage();
            process::exit(2);
        }
    }
}
