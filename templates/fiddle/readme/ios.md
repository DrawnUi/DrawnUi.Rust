# My App

Made with [DrawFiddle](https://drawfiddle.com) and [DrawnUI for Rust](https://github.com/DrawnUi/DrawnUi.Rust),
a UI engine that draws everything with Skia. Your code is in `src/app.rs`; `src/lib.rs` starts it
with the fonts DrawFiddle uses.

You need [Rust](https://rustup.rs) 1.94 or newer. Nothing for Skia: the first build downloads it
prebuilt for your platform. API docs: https://docs.rs/drawnui

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
