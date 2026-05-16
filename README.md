# ram-monitor

macOS system daemon (written in Rust) that kills the largest-RAM processes when system memory usage exceeds a configurable threshold (default 50 GB). Runs as root at boot, before any user login.

Released as a signed + notarized universal DMG when CI has signing creds; otherwise as an unsigned DMG with the same layout.

## What it does

Every `RAM_INTERVAL_SECS` seconds the daemon inspects total used memory. If `used ≥ RAM_THRESHOLD_GB`, it enumerates all processes, sorts by RSS descending, and sends `SIGTERM` to the largest processes (skipping `pid ≤ 1`, itself, and a small allowlist of critical macOS daemons) until projected used memory drops below the threshold.

Terminals and IDEs are **not** protected — they're the most common culprits for runaway RAM, so they're fair game.

## Install (from a release DMG)

1. Download `ram-monitor.dmg` from the [Releases](../../releases) page.
2. Open it.
3. Drag **ram-monitor.app** to **Applications**.
4. Double-click the installed app once. You'll get a native macOS admin-password prompt; on accept it copies the LaunchDaemon plist into `/Library/LaunchDaemons/` and bootstraps the daemon. From the next boot onward the daemon runs as root before login.

Or, equivalently from a terminal: `sudo /Applications/ram-monitor.app/Contents/MacOS/ram-monitor install`.

## Uninstall

```bash
sudo /Applications/ram-monitor.app/Contents/MacOS/ram-monitor uninstall
```

## Configuration

| Env var | Default | Meaning |
|---|---|---|
| `RAM_THRESHOLD_GB` | `50` | Used-RAM threshold in GB. |
| `RAM_INTERVAL_SECS` | `5` | Poll interval in seconds. |
| `RAM_DRY_RUN` | `0` | If `1`, logs would-kill decisions but does not signal anything. |

These live in the `EnvironmentVariables` block of `/Library/LaunchDaemons/com.swack.ram-monitor.plist` after install. Edit, then:

```bash
sudo launchctl bootout  system/com.swack.ram-monitor
sudo launchctl bootstrap system /Library/LaunchDaemons/com.swack.ram-monitor.plist
```

## Logs

```
/Library/Logs/ram-monitor/ram-monitor.log
/Library/Logs/ram-monitor/ram-monitor.err.log
```

Tail live: `sudo tail -f /Library/Logs/ram-monitor/ram-monitor.log`.

## Build from source

Requires: Rust stable, macOS 11+. Optionally [`just`](https://github.com/casey/just).

```bash
just build       # cargo build --release
just test        # cargo test
just build-dmg   # universal .app + DMG via scripts/make-dmg.sh
just install     # build, copy .app to /Applications, register daemon
just uninstall   # remove daemon + .app
just logs        # sudo tail -f the log
just status      # launchctl print system/com.swack.ram-monitor
```

Or directly:

```bash
cargo build --release
bash scripts/make-dmg.sh        # produces ram-monitor.dmg
```

## Protected processes

Never killed: `launchd`, `kernel_task`, `WindowServer`, `loginwindow`, `Finder`, `Dock`, `SystemUIServer`, `ControlCenter`, `NotificationCenter`, `coreaudiod`, `powerd`, `configd`, `logd`, `opendirectoryd`, `securityd`, `trustd`, `mds`, `mds_stores`, `cfprefsd`, `distnoted`, `UserEventAgent`, `syspolicyd`, `tccd`, `sandboxd`, `WindowManager`, `sshd`, `sshd-session`, and `ram-monitor` itself.

## Releases

Pre-built signed + notarized DMGs (universal x86_64 + arm64) are published to [GitHub Releases](../../releases) on every `vX.Y.Z` tag. Each release attaches `ram-monitor.dmg` and its `ram-monitor.dmg.sha256`. The build is driven by `.github/workflows/release.yml` → `scripts/make-dmg.sh`.

## License

GPL-3.0-or-later — see [`LICENSE`](./LICENSE).
