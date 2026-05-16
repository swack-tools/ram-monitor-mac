#!/usr/bin/env bash
# Build ram-monitor.dmg from source.
#
# Steps:
#   1. cargo build --release for x86_64 + arm64 → lipo into universal binary
#   2. Assemble ram-monitor.app/Contents/{MacOS,Resources,Info.plist}
#   3. Build .icns from assets/icon-1024.png
#   4. (optional) codesign + notarize when signing env vars are present
#   5. hdiutil create the DMG with an /Applications symlink for drag-drop
#
# Env vars (all optional — missing ones cause that stage to be skipped):
#   VERSION                      defaults to "dev"
#   BUILD_NUMBER                 defaults to git rev-count or "1"
#   APPLE_TEAM_NAME              e.g. "SWACKTECH, LLC"
#   APPLE_TEAM_ID                10-char team id
#   APPLE_ID                     Apple ID email
#   APPLE_APP_PASSWORD           app-specific password
#   APPLE_CERTIFICATE_BASE64     base64-encoded .p12 (CI only — local builds use
#                                identities already in the keychain)
#   APPLE_CERTIFICATE_PASSWORD   password for the .p12
#
# Output: ./ram-monitor.dmg + ./ram-monitor.dmg.sha256

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/.." && pwd)"
cd "${ROOT}"

VERSION="${VERSION:-dev}"
BUILD_NUMBER="${BUILD_NUMBER:-$(git rev-list --count HEAD 2>/dev/null || echo 1)}"
APP_NAME="ram-monitor.app"
APP_PATH="${ROOT}/${APP_NAME}"
DMG_PATH="${ROOT}/ram-monitor.dmg"
STAGING="${ROOT}/dmg_staging"

step() { printf "\n==> %s\n" "$*"; }

CAN_SIGN=0
if [[ -n "${APPLE_TEAM_NAME:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
    if [[ -n "${APPLE_CERTIFICATE_BASE64:-}" && -n "${APPLE_CERTIFICATE_PASSWORD:-}" ]]; then
        CAN_SIGN=1
    elif security find-identity -v -p codesigning 2>/dev/null \
            | grep -q "Developer ID Application: ${APPLE_TEAM_NAME} (${APPLE_TEAM_ID})"; then
        CAN_SIGN=1
    fi
fi
CAN_NOTARIZE=0
if [[ "${CAN_SIGN}" -eq 1 && -n "${APPLE_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]; then
    CAN_NOTARIZE=1
fi

step "Build universal binary (cargo + lipo)"
rustup target list --installed | grep -q '^x86_64-apple-darwin$'   || rustup target add x86_64-apple-darwin
rustup target list --installed | grep -q '^aarch64-apple-darwin$'  || rustup target add aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
mkdir -p dist
lipo -create -output dist/ram-monitor \
    target/x86_64-apple-darwin/release/ram-monitor \
    target/aarch64-apple-darwin/release/ram-monitor
file dist/ram-monitor

step "Build .icns"
bash assets/build_icns.sh

step "Assemble ${APP_NAME}"
rm -rf "${APP_PATH}"
mkdir -p "${APP_PATH}/Contents/MacOS" "${APP_PATH}/Contents/Resources"
cp dist/ram-monitor "${APP_PATH}/Contents/MacOS/ram-monitor"
chmod +x "${APP_PATH}/Contents/MacOS/ram-monitor"
cp assets/icon.icns "${APP_PATH}/Contents/Resources/icon.icns"
cat > "${APP_PATH}/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleExecutable</key><string>ram-monitor</string>
    <key>CFBundleIdentifier</key><string>com.swack.ram-monitor</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>ram-monitor</string>
    <key>CFBundleDisplayName</key><string>RAM Monitor</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key><string>${BUILD_NUMBER}</string>
    <key>CFBundleIconFile</key><string>icon</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Copyright © swack-tools.</string>
</dict>
</plist>
EOF
plutil -lint "${APP_PATH}/Contents/Info.plist" >/dev/null

if [[ "${CAN_SIGN}" -eq 1 ]]; then
    step "Codesign .app (Developer ID, hardened runtime, timestamp)"
    if [[ -n "${APPLE_CERTIFICATE_BASE64:-}" ]]; then
        # CI path: import cert into a temp keychain
        KEYCHAIN_PASSWORD="$(openssl rand -base64 32)"
        security create-keychain -p "${KEYCHAIN_PASSWORD}" build.keychain
        echo "${APPLE_CERTIFICATE_BASE64}" | base64 --decode > certificate.p12
        security import certificate.p12 -k build.keychain -P "${APPLE_CERTIFICATE_PASSWORD}" -T /usr/bin/codesign
        security list-keychains -d user -s build.keychain $(security list-keychains -d user | tr -d '"')
        security unlock-keychain -p "${KEYCHAIN_PASSWORD}" build.keychain
        security set-key-partition-list -S apple-tool:,apple: -s -k "${KEYCHAIN_PASSWORD}" build.keychain
        rm -f certificate.p12
        trap 'security delete-keychain build.keychain >/dev/null 2>&1 || true' EXIT
    fi
    IDENTITY="Developer ID Application: ${APPLE_TEAM_NAME} (${APPLE_TEAM_ID})"
    codesign --force --options runtime --timestamp --sign "${IDENTITY}" \
        "${APP_PATH}/Contents/MacOS/ram-monitor"
    codesign --force --options runtime --timestamp --sign "${IDENTITY}" \
        "${APP_PATH}"
    codesign -dvv "${APP_PATH}"
else
    step "Skipping codesign (signing credentials not configured)"
fi

step "Stage DMG contents"
rm -rf "${STAGING}"
mkdir -p "${STAGING}"
cp -R "${APP_PATH}" "${STAGING}/${APP_NAME}"
ln -s /Applications "${STAGING}/Applications"
ls -la "${STAGING}"

step "Create DMG"
rm -f "${DMG_PATH}"
hdiutil create \
    -volname "ram-monitor ${VERSION}" \
    -srcfolder "${STAGING}" \
    -ov \
    -format UDZO \
    -imagekey zlib-level=9 \
    "${DMG_PATH}"
ls -lh "${DMG_PATH}"

if [[ "${CAN_SIGN}" -eq 1 ]]; then
    step "Codesign DMG"
    codesign --force --timestamp --sign "Developer ID Application: ${APPLE_TEAM_NAME} (${APPLE_TEAM_ID})" "${DMG_PATH}"
    codesign -dvv "${DMG_PATH}"
fi

if [[ "${CAN_NOTARIZE}" -eq 1 ]]; then
    step "Notarize DMG"
    xcrun notarytool submit "${DMG_PATH}" \
        --apple-id "${APPLE_ID}" \
        --password "${APPLE_APP_PASSWORD}" \
        --team-id  "${APPLE_TEAM_ID}" \
        --wait
    xcrun stapler staple "${DMG_PATH}"
    xcrun stapler validate "${DMG_PATH}"
else
    step "Skipping notarization (creds not configured)"
fi

step "Checksum"
shasum -a 256 "${DMG_PATH}" > "${DMG_PATH}.sha256"
cat "${DMG_PATH}.sha256"

step "Done"
echo "DMG:      ${DMG_PATH}"
echo "SHA256:   ${DMG_PATH}.sha256"
[[ "${CAN_SIGN}" -eq 1 ]]    && echo "Signed:   yes"    || echo "Signed:   no"
[[ "${CAN_NOTARIZE}" -eq 1 ]] && echo "Notarized: yes" || echo "Notarized: no"
