# android-activity `drawnui-null-text`: the patch DrawnUi.Rust builds against

The root `Cargo.toml` patches android-activity onto a sibling checkout `../android-activity`
(branch `drawnui-null-text`): release 0.6.1 plus this commit (no slice from a null text pointer
before GameActivity's first text state; `solved-android.md`). Cargo needs the checkout on every
machine, also where Android is not built. Until the fix is upstream, make it from upstream:

```sh
cd <folder that holds DrawnUi.Rust>
git clone https://github.com/rust-mobile/android-activity.git
cd android-activity
git checkout -b drawnui-null-text b4ddf059b77be12cdb955d394e65a92e7568d936   # tag v0.6.1
git am ../DrawnUi.Rust/dev/android-activity-null-text/*.patch
```
