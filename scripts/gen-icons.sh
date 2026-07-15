#!/usr/bin/env bash
# Regenerate every logo asset from the raw master (logo.png at repo root):
#   - public/logo.png        rounded, full-bleed web copy (favicon + in-app + README)
#   - src-tauri/app-icon.png  rounded + padded macOS-style OS master
#   - src-tauri/icons/*       full desktop icon set, generated from that master
# Run from anywhere after replacing logo.png. Requires ImageMagick 7 (magick).
set -euo pipefail

cd "$(dirname "$0")/.."
SRC=logo.png
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Rounded-rect radii are ~22% of edge, the macOS icon-grid corner ratio.

# Web copy: full-bleed rounded corners, no padding (favicons are tiny).
magick -size 1080x1080 xc:black -fill white \
  -draw "roundrectangle 0,0,1079,1079,238,238" "$TMP/mask_web.png"
mkdir -p public
magick "$SRC" "$TMP/mask_web.png" -alpha off -compose CopyOpacity -composite \
  -resize 512x512 public/logo.png

# OS master: art at ~80% inside a transparent 1024 canvas, matching macOS padding.
magick -size 824x824 xc:black -fill white \
  -draw "roundrectangle 0,0,823,823,185,185" "$TMP/mask_mac.png"
magick "$SRC" -resize 824x824 "$TMP/mask_mac.png" -alpha off \
  -compose CopyOpacity -composite "$TMP/rounded_mac.png"
magick "$TMP/rounded_mac.png" -background none -gravity center \
  -extent 1024x1024 src-tauri/app-icon.png

npm run tauri icon src-tauri/app-icon.png

# Desktop app: drop the mobile sets `tauri icon` also emits.
rm -rf src-tauri/icons/android src-tauri/icons/ios

echo "Icons regenerated. Rebuild the app (tauri build / tauri dev) to pick up the OS icon."
