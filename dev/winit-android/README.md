# winit `drawnui-android`: the patch DrawnUi.Rust builds against

The root `Cargo.toml` patches winit onto a sibling checkout `../winit` (branch `drawnui-android`):
winit 0.30.13 plus this commit (Android: the event loop ends on the activity's Destroy, a new loop
may be built for the next activity, insets changes arrive as a same-size `Resized`;
`solved-android.md` 21, 22). Cargo needs the checkout on every machine, also where Android is not
built (the change is Android-only). Until it is upstream, make it from upstream:

```sh
cd <folder that holds DrawnUi.Rust>
git clone https://github.com/rust-windowing/winit.git
cd winit
git checkout -b drawnui-android e9809ef54b18499bb4f2cac945719ecc2a61061b   # tag v0.30.13
git am ../DrawnUi.Rust/dev/winit-android/*.patch
```
