#!/usr/bin/env bash
# Android: builds libmyapp.so with cargo-ndk, packs the APK with Gradle (android/, GameActivity) and,
# with --install, installs and starts it on the phone or emulator adb sees.
#   ./android.sh                        -> arm64-v8a (phones): target/android/myapp.apk
#   ./android.sh x86_64 --install       -> the emulator
#   ./android.sh all                    -> arm64-v8a, armeabi-v7a, x86_64 and x86 in one APK
# Needs: the Android SDK (ANDROID_HOME) with platform 36, NDK 26, a JDK 17 or newer (JAVA_HOME),
# cargo-ndk (cargo install cargo-ndk) and the Rust Android targets.
set -euo pipefail
cd "$(dirname "$0")"
abi=arm64-v8a
install=0
for arg in "$@"; do
    case "$arg" in
        --install) install=1 ;;
        *) abi=$arg ;;
    esac
done
# NDK 26: the prebuilt Skia is built with it, and the app links Skia's C++ library from it.
# ANDROID_NDK_HOME is used when it is a 26, else the SDK's NDK 26.
case "$(basename "${ANDROID_NDK_HOME:-none}")" in
    26.*) ;;
    *)
        ANDROID_NDK_HOME=$(ls -d "$ANDROID_HOME"/ndk/26.* 2>/dev/null | tail -1)
        [ -n "$ANDROID_NDK_HOME" ] || { echo "NDK 26 not found: install it with the SDK Manager (ndk;26.1.10909125)" >&2; exit 1; }
        export ANDROID_NDK_HOME
        ;;
esac
if [ "$abi" = all ]; then abis="arm64-v8a armeabi-v7a x86_64 x86"; else abis=$abi; fi
targets=()
for a in $abis; do targets+=(-t "$a"); done

# Only this build's libraries go into the APK.
rm -rf target/android/lib target/android/stage
cargo ndk "${targets[@]}" --platform 26 -o target/android/lib rustc --lib --release --crate-type cdylib
# The app reads its files as "assets/...": they go to the APK as assets/assets/...
mkdir -p target/android/stage
cp -R assets target/android/stage/assets

chmod +x android/gradlew
android/gradlew -p android --project-cache-dir "$(pwd)/target/android/gradle-cache" -q assembleRelease
cp target/android/gradle/outputs/apk/release/myapp-release.apk target/android/myapp.apk
echo "APK: $(pwd)/target/android/myapp.apk"

if [ $install = 1 ]; then
    adb install -r target/android/myapp.apk
    adb shell am start -n com.example.myapp/com.google.androidgamesdk.GameActivity
fi
