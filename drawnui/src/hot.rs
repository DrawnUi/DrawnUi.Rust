//! Hot reload (feature `hot-reload`): an app built as a side module is swapped into the running
//! engine and starts with the state of the app it replaces.
//!
//! The app crate declares its entry points with [`hot_reload!`](crate::hot_reload); the browser host
//! loads a new module with `dui_hot_swap`. The state crosses as JSON, so the new code may add,
//! remove or change fields: a field the saved state lacks takes its default (`#[serde(default)]` on
//! the struct), and a state that no longer reads as the new type starts over from `Default`.
//!
//! Link the side module with `-Bsymbolic`: the previous module stays loaded and the loader keeps the
//! first definition of each symbol, so without it the new module runs the previous module's code
//! for every function with the same name (all generic code over `App`). See `hotreload.md`.

use crate::{App, Ui};

/// The state of `app` as JSON, when `app` is a `Ui<S>`.
pub fn save<S: serde::Serialize + 'static>(app: &dyn App) -> Option<String> {
    let ui = app.as_any()?.downcast_ref::<Ui<S>>()?;
    match serde_json::to_string(&ui.state) {
        Ok(json) => Some(json),
        Err(e) => {
            eprintln!("drawnui: hot reload: the state was not saved: {e}");
            None
        }
    }
}

/// The state saved by [`save`], or `S::default()` when there is none or it no longer reads as `S`.
pub fn restore<S: serde::de::DeserializeOwned + Default>(saved: Option<&str>) -> S {
    let Some(json) = saved else { return S::default() };
    serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("drawnui: hot reload: the saved state does not fit the new code, starting over: {e}");
        S::default()
    })
}

/// Declares the hot reload entry points of an app built as a side module: `state` is the app
/// state type (`Default`, `Serialize`, `Deserialize`), `make` turns a state into the app.
///
/// ```ignore
/// #[derive(Default, serde::Serialize, serde::Deserialize)]
/// #[serde(default)]
/// struct App { count: i32 }
///
/// drawnui::hot_reload!(App, |state| Ui::new(state, build).font("Default", "OpenSans-Regular.ttf"));
/// ```
#[macro_export]
macro_rules! hot_reload {
    ($state:ty, $make:expr) => {
        #[unsafe(no_mangle)]
        pub fn drawnui_hot_make(saved: ::core::option::Option<&str>) -> ::std::boxed::Box<dyn $crate::App> {
            let state: $state = $crate::hot::restore(saved);
            ::std::boxed::Box::new(($make)(state))
        }

        #[unsafe(no_mangle)]
        pub fn drawnui_hot_save(app: &dyn $crate::App) -> ::core::option::Option<::std::string::String> {
            $crate::hot::save::<$state>(app)
        }
    };
}
