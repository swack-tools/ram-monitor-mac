#!/usr/bin/env bash
# Build assets/icon.icns from assets/icon-1024.png using macOS's iconutil.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="${HERE}/icon-1024.png"
ICONSET="${HERE}/icon.iconset"
OUT="${HERE}/icon.icns"

if [[ ! -f "${SRC}" ]]; then
    echo "regenerating ${SRC}"
    python3 "${HERE}/generate_icon.py"
fi

rm -rf "${ICONSET}"
mkdir -p "${ICONSET}"

# size_label:px_dimension pairs (label used by iconutil)
declare -a SIZES=(
    "icon_16x16:16"
    "icon_16x16@2x:32"
    "icon_32x32:32"
    "icon_32x32@2x:64"
    "icon_128x128:128"
    "icon_128x128@2x:256"
    "icon_256x256:256"
    "icon_256x256@2x:512"
    "icon_512x512:512"
    "icon_512x512@2x:1024"
)

for entry in "${SIZES[@]}"; do
    label="${entry%%:*}"
    px="${entry##*:}"
    sips -z "${px}" "${px}" "${SRC}" --out "${ICONSET}/${label}.png" >/dev/null
done

iconutil -c icns "${ICONSET}" -o "${OUT}"
ls -lh "${OUT}"
echo "wrote ${OUT}"
