//! iOS: the scene lifecycle (UIScene). An app linked with the iOS 27 SDK must adopt it or UIKit
//! stops it at launch, whatever iOS version the device runs. The app's Info.plist names this
//! module's scene delegate, `DrawnUiSceneDelegate`, in its `UIApplicationSceneManifest`; UIKit
//! then makes one window scene and this delegate puts the app's window in it. winit makes its
//! window before the scene connects and takes the app's lifecycle (active, background) from the
//! UIApplication notifications, which UIKit still posts with scenes: nothing else changes.
//! An app without the manifest runs as before (its window needs no scene).

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::{ClassType, MainThreadOnly, Message, define_class};
use objc2_ui_kit::{UIResponder, UIScene, UISceneConnectionOptions, UISceneDelegate, UISceneSession, UIView, UIWindow, UIWindowScene, UIWindowSceneDelegate};

thread_local! {
    /// The app's window scene, once UIKit connected it.
    static SCENE: RefCell<Option<Retained<UIWindowScene>>> = const { RefCell::new(None) };
    /// The app's windows, each put in the scene when there is one.
    static WINDOWS: RefCell<Vec<Retained<UIWindow>>> = const { RefCell::new(Vec::new()) };
}

define_class!(
    /// The scene delegate the app's Info.plist names (`UISceneDelegateClassName`).
    #[unsafe(super(UIResponder))]
    #[thread_kind = MainThreadOnly]
    #[name = "DrawnUiSceneDelegate"]
    struct SceneDelegate;

    unsafe impl NSObjectProtocol for SceneDelegate {}

    unsafe impl UISceneDelegate for SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn will_connect(&self, scene: &UIScene, _session: &UISceneSession, _options: &UISceneConnectionOptions) {
            let Some(scene) = scene.downcast_ref::<UIWindowScene>() else { return };
            let scene = scene.retain();
            WINDOWS.with(|windows| windows.borrow().iter().for_each(|window| place(window, &scene)));
            SCENE.with(|s| *s.borrow_mut() = Some(scene));
        }
    }

    unsafe impl UIWindowSceneDelegate for SceneDelegate {}
);

fn place(window: &UIWindow, scene: &UIWindowScene) {
    window.setWindowScene(Some(scene));
    window.makeKeyAndVisible();
}

/// Makes the delegate class known to the Objective-C runtime, before UIKit looks it up by name.
pub(crate) fn register() {
    let _ = SceneDelegate::class();
}

/// The window winit made for the app: into the scene now, or when it connects.
pub(crate) fn adopt(window: &winit::window::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(RawWindowHandle::UiKit(handle)) = window.window_handle().map(|h| h.as_raw()) else { return };
    // SAFETY: winit's view lives as long as its window; this runs on the main thread.
    let view = unsafe { &*(handle.ui_view.as_ptr() as *const UIView) };
    let Some(ui_window) = view.window() else {
        eprintln!("drawnui: iOS: the app's view has no window to put in the scene");
        return;
    };
    if let Some(scene) = SCENE.with(|s| s.borrow().clone()) {
        place(&ui_window, &scene);
    }
    WINDOWS.with(|w| w.borrow_mut().push(ui_window));
}
