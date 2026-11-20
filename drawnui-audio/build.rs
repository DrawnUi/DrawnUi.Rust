// The browser build plays through emscripten's OpenAL (Web Audio underneath): link it.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("emscripten") {
        println!("cargo:rustc-link-lib=openal");
    }
}
