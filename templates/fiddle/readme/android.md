# My App

Made with [DrawFiddle](https://drawfiddle.com) and [DrawnUI for Rust](https://github.com/DrawnUi/DrawnUi.Rust),
a UI engine that draws everything with Skia. Your code is in `src/app.rs`; `src/lib.rs` starts it
with the fonts DrawFiddle uses.

You need [Rust](https://rustup.rs) 1.94 or newer. Nothing for Skia: the first build downloads it
prebuilt for your platform. API docs: https://docs.rs/drawnui

## Android

Needs:

- the Android SDK with platform 36 and NDK 26 (Android Studio's SDK Manager, or
  `sdkmanager "platforms;android-36" "ndk;26.1.10909125"`), `ANDROID_HOME` set;
- a JDK 17 or newer (`JAVA_HOME`; Android Studio's `jbr` folder works);
- cargo-ndk and the Rust Android targets:

```
cargo install cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
```

Build the APK (phones, arm64), or build and start it on the emulator:

```
./android.sh                       # Windows PowerShell: .\android.ps1
./android.sh x86_64 --install      # Windows PowerShell: .\android.ps1 -Abi x86_64 -Install
./android.sh all                   # every ABI in one APK: arm64, armv7, x86_64, x86
```

The APK is `target/android/myapp.apk`, signed with the debug key so it installs right away; sign
it with your own key to publish. The Gradle project is `android/` (GameActivity), the package is
`com.example.myapp`.

`Cargo.toml` takes winit and android-activity with one fix each from GitHub until the fixes are
released, so the first build also needs git.
