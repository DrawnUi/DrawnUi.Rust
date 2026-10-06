# My App

Made with [DrawFiddle](https://drawfiddle.com) and [DrawnUI for Rust](https://github.com/DrawnUi/DrawnUi.Rust),
a UI engine that draws everything with Skia. Your code is in `src/app.rs`; `src/lib.rs` starts it
with the fonts DrawFiddle uses.

You need [Rust](https://rustup.rs) 1.94 or newer. Nothing for Skia: the first build downloads it
prebuilt for your platform. API docs: https://docs.rs/drawnui

One project for every platform: Windows, macOS, Linux, the web, Android and iOS.

## Windows

Needs the Visual Studio C++ build tools ("Desktop development with C++", or the free Build Tools).

```
cargo run --release
```

The app is `target\release\myapp.exe`, with its `assets` folder next to it.

## macOS

```
cargo run --release
```

The app is `target/release/myapp`, drawn with Metal.

## Linux

Needs a C toolchain and a few development packages. On Debian or Ubuntu:

```
sudo apt install build-essential pkg-config libfontconfig1-dev libx11-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev libegl1-mesa-dev
cargo run --release
```

The app is `target/release/myapp`, with its `assets` folder and `icon.ico` next to it. It runs on
X11 and Wayland with OpenGL.

## Web

Needs the WebAssembly target and Emscripten 6.0.10:

```
rustup target add wasm32-unknown-emscripten
git clone https://github.com/emscripten-core/emsdk.git
cd emsdk
./emsdk install 6.0.10
./emsdk activate 6.0.10
source ./emsdk_env.sh        # Windows PowerShell: .\emsdk_env.ps1
```

Then, in this folder:

```
./web.sh                     # Windows PowerShell: .\web.ps1
python3 -m http.server 8080 -d dist
```

Open http://localhost:8080. `dist/` is the whole site (the page, `myapp.js` / `myapp.wasm`, the
assets and `drawnui_host.js`): upload it to any static host.

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

## iOS

Needs a Mac with Xcode and the Rust iOS targets:

```
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
```

Run it on the simulator, or on your iPhone:

```
ios/ios.sh
ios/ios.sh device
```

On a device the app is signed with your Apple developer account: sign in to Xcode once and have a
development profile for `com.example.myapp` (the top of `ios/ios.sh` says how).
