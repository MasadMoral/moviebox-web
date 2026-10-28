#!/usr/bin/env bash
# Regenerate the Android launcher icons from the SVG sources.
# Requires rsvg-convert (librsvg). Run from anywhere: android-app/make-icons.sh
set -euo pipefail
cd "$(dirname "$0")"

command -v rsvg-convert >/dev/null || {
  echo "rsvg-convert is required (Arch: sudo pacman -S librsvg)" >&2
  exit 1
}

RES=overlay/android/app/src/main/res

for spec in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
  density=${spec%%:*}
  size=${spec##*:}
  dir="$RES/mipmap-$density"
  mkdir -p "$dir"
  rsvg-convert -w "$size" -h "$size" icon.svg -o "$dir/ic_launcher.png"
  rsvg-convert -w "$size" -h "$size" icon-round.svg -o "$dir/ic_launcher_round.png"
done

for spec in mdpi:108 hdpi:162 xhdpi:216 xxhdpi:324 xxxhdpi:432; do
  density=${spec%%:*}
  size=${spec##*:}
  mkdir -p "$RES/mipmap-$density"
  rsvg-convert -w "$size" -h "$size" icon-foreground.svg \
    -o "$RES/mipmap-$density/ic_launcher_foreground.png"
done

echo "Icons written to $RES"
