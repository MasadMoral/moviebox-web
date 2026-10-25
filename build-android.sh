#!/usr/bin/env bash
# Builds moviebox-web.apk (arm64) locally. Run from the repo root: ./build-android.sh
set -euo pipefail
cd "$(dirname "$0")"

: "${ANDROID_HOME:?Set ANDROID_HOME, e.g. export ANDROID_HOME=\$HOME/android-sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"
echo "Using NDK: $ANDROID_NDK_HOME"

rm -rf jniLibs app

# 1. Rust server -> native library for Android
cargo ndk -t arm64-v8a -o jniLibs build --release --lib

# 2. Flutter shell + our files
flutter create --platforms=android --org com.masad --project-name moviebox app
rm -rf app/test
cp -r android-app/overlay/. app/
mkdir -p app/android/app/src/main/jniLibs
cp -r jniLibs/. app/android/app/src/main/jniLibs/

M=app/android/app/src/main/AndroidManifest.xml
android-app/patch-manifest.sh "$M"

# 3. APK
(cd app && flutter pub get && flutter build apk --release --target-platform android-arm64)
cp app/build/app/outputs/flutter-apk/app-release.apk moviebox-web.apk
echo
echo "Done: $PWD/moviebox-web.apk"
echo "Install on a USB-connected phone: adb install -r moviebox-web.apk"
