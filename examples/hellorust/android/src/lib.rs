//! HelloRust on Android: the activity starts the app here.

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    drawnui::set_android_app(app);
    // `adb shell setprop debug.drawnui.bench 2000,mesh`: the bench scene instead of HelloRust, for
    // measurements on a device (`off` or no number: HelloRust).
    match property("debug.drawnui.bench") {
        Some(args) if args.starts_with(|c: char| c.is_ascii_digit()) => bench::run(args.split([',', ' ']).map(String::from)),
        _ => hellorust::run(),
    }
}

/// An Android system property; `None` when unset.
#[cfg(target_os = "android")]
fn property(name: &str) -> Option<String> {
    use std::ffi::{CStr, CString, c_char, c_int};
    unsafe extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
    }
    let name = CString::new(name).ok()?;
    let mut value = [0 as c_char; 92]; // PROP_VALUE_MAX
    // SAFETY: a NUL-terminated name and a buffer of PROP_VALUE_MAX bytes.
    let len = unsafe { __system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    (len > 0).then(|| unsafe { CStr::from_ptr(value.as_ptr()) }.to_string_lossy().into_owned())
}
