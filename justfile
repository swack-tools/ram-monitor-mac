# ram-monitor task runner
# Run `just --list` to see all recipes.

set shell := ["bash", "-cu"]

DAEMON_LABEL := "com.swack.ram-monitor"
LOG          := "/Library/Logs/ram-monitor/ram-monitor.log"
APP_DEST     := "/Applications/ram-monitor.app"

# Default: list recipes
default:
    @just --list

# Build a release binary for the host arch.
build:
    cargo build --release

# Run the test suite.
test:
    cargo test

# Build the universal, signed/notarized (when creds present) DMG.
build-dmg:
    bash scripts/make-dmg.sh

# Local-install pipeline: build .app bundle, copy to /Applications, register
# the LaunchDaemon. Sudo-elevates as needed.
install: build
    bash scripts/make-dmg.sh
    sudo rm -rf "{{APP_DEST}}"
    sudo cp -R ram-monitor.app "{{APP_DEST}}"
    sudo "{{APP_DEST}}/Contents/MacOS/ram-monitor" install

# Uninstall the daemon system-wide.
uninstall:
    sudo "{{APP_DEST}}/Contents/MacOS/ram-monitor" uninstall || true

# Tail the daemon log.
logs:
    sudo tail -f {{LOG}}

# Show launchctl status of the daemon.
status:
    sudo launchctl print system/{{DAEMON_LABEL}} | head -30

# Clean build artifacts.
clean:
    cargo clean
    rm -rf dist dmg_staging ram-monitor.app ram-monitor.dmg ram-monitor.dmg.sha256
    rm -rf assets/icon.iconset assets/icon.icns
