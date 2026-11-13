//! Dungeon Run on Android: the activity starts the game here.

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    // The scene's density cap for tries: adb shell am start -n <package>/<activity> --ef maxdensity 1.5
    // (0 = no cap).
    if let Some(max_density) = intent_max_density(&app) {
        // SAFETY: set before the game starts, while no other thread reads the environment.
        unsafe { std::env::set_var("DUNGEON_MAXDENSITY", max_density.to_string()) };
    }
    drawnui::set_android_app(app);
    dungeon::run();
}

/// The activity intent's float extra `maxdensity`, when it has one (0 or more).
#[cfg(target_os = "android")]
fn intent_max_density(app: &drawnui::AndroidApp) -> Option<f32> {
    use jni_sys::{JNI_OK, JNIEnv, JavaVM, jobject, jvalue};
    use std::ptr::null_mut;
    // SAFETY: the VM and activity pointers come from android-activity for this process; every
    // call is checked, the local references are freed, and a pending Java exception is cleared.
    unsafe {
        let vm = app.vm_as_ptr() as *mut JavaVM;
        let mut env: *mut JNIEnv = null_mut();
        if ((**vm).AttachCurrentThread?)(vm, (&mut env as *mut *mut JNIEnv).cast(), null_mut()) != JNI_OK {
            return None;
        }
        let f = &**env;
        let activity = app.activity_as_ptr() as jobject;
        let class = (f.GetObjectClass?)(env, activity);
        let get_intent = (f.GetMethodID?)(env, class, c"getIntent".as_ptr(), c"()Landroid/content/Intent;".as_ptr());
        let intent = (f.CallObjectMethodA?)(env, activity, get_intent, std::ptr::null());
        (f.DeleteLocalRef?)(env, class);
        if intent.is_null() || (f.ExceptionCheck?)(env) != 0 {
            (f.ExceptionClear?)(env);
            return None;
        }
        let intent_class = (f.GetObjectClass?)(env, intent);
        let get_float = (f.GetMethodID?)(env, intent_class, c"getFloatExtra".as_ptr(), c"(Ljava/lang/String;F)F".as_ptr());
        let key = (f.NewStringUTF?)(env, c"maxdensity".as_ptr());
        let args = [jvalue { l: key }, jvalue { f: -1.0 }];
        let max_density = (f.CallFloatMethodA?)(env, intent, get_float, args.as_ptr());
        let failed = (f.ExceptionCheck?)(env) != 0;
        if failed {
            (f.ExceptionClear?)(env);
        }
        (f.DeleteLocalRef?)(env, key);
        (f.DeleteLocalRef?)(env, intent_class);
        (f.DeleteLocalRef?)(env, intent);
        (!failed && max_density >= 0.0).then_some(max_density)
    }
}
