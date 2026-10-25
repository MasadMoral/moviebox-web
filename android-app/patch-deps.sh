#!/usr/bin/env bash
# flutter_inappwebview_android 1.1.3 still references proguard-android.txt,
# which newer Android Gradle Plugin versions reject. Switch it to the
# optimized ProGuard defaults before Gradle configures the plugin.
# Safe to run repeatedly; applies to whichever cached plugin version is present.
set -euo pipefail

shopt -s nullglob
files=("${HOME}"/.pub-cache/hosted/pub.dev/flutter_inappwebview_android-*/android/build.gradle)
if [ "${#files[@]}" -eq 0 ]; then
  echo "flutter_inappwebview_android is not in the pub cache; run flutter pub get first" >&2
  exit 1
fi
for f in "${files[@]}"; do
  sed -i 's/proguard-android\.txt/proguard-android-optimize.txt/g' "$f"
  echo "Patched $f"
done
