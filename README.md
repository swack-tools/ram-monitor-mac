# ram-monitor

macOS background service that kills the largest-RAM processes when system memory usage exceeds a configurable threshold (default 50GB).

## Behavior

Every `RAM_INTERVAL_SECS` seconds the service inspects total used memory. If `used >= RAM_THRESHOLD_GB`, it enumerates all processes, sorts by RSS descending, and sends `SIGTERM` to the largest processes (skipping itself, pid<=1, and a small allowlist of critical macOS daemons) until projected used memory falls below the threshold.

## Env vars

| Var | Default | Meaning |
|-----|---------|---------|
| `RAM_THRESHOLD_GB` | `50` | Used-RAM threshold in GB. |
| `RAM_INTERVAL_SECS` | `5` | Poll interval in seconds. |
| `RAM_DRY_RUN` | `0` | If `1`, logs would-kill decisions but does not signal anything. |

## Install

```bash
cargo build --release
cp com.swack.ram-monitor.plist ~/Library/LaunchAgents/
launchctl load -w ~/Library/LaunchAgents/com.swack.ram-monitor.plist
```

## Logs

```
~/Library/Logs/ram-monitor/ram-monitor.log
~/Library/Logs/ram-monitor/ram-monitor.err.log
```

## Unload

```bash
launchctl unload ~/Library/LaunchAgents/com.swack.ram-monitor.plist
```

## Protected processes

The allowlist (never killed) covers: `launchd`, `kernel_task`, `WindowServer`, `loginwindow`, `Finder`, `Dock`, `SystemUIServer`, `ControlCenter`, `NotificationCenter`, `coreaudiod`, `powerd`, `configd`, `logd`, `opendirectoryd`, `securityd`, `trustd`, `mds`, `mds_stores`, `cfprefsd`, `distnoted`, `UserEventAgent`, `syspolicyd`, `tccd`, `sandboxd`, `WindowManager`, `sshd`, `sshd-session`, and `ram-monitor` itself.

Terminals and IDEs are NOT protected (they are the most common culprits for high RAM).
