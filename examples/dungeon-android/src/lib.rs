//! Dungeon Run on Android: the activity starts the game here.

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    drawnui::set_android_app(app);
    dungeon::run();
}
