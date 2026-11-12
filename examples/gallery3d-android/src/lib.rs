//! The 3D gallery on Android: the activity starts the app here.

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    drawnui::set_android_app(app);
    gallery3d::run();
}
