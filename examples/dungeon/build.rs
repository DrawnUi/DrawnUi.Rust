use std::{env, fs, path::Path};

// The exe's icon (resource 1): Explorer shows it, and drawnui shows it on the window and the taskbar.
// The assets go next to the exe: drawnui reads them from there first, so the app also runs when it
// is started from Explorer or a shortcut (another working folder).
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-changed=assets");
    // A build script runs on the build machine: embed-resource looks at that one, not at the
    // target, and would link a Windows resource into the wasm build. The web build has a favicon
    // and copies its assets into its own output folder.
    let target = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target == "emscripten" {
        return;
    }
    if target == "windows" {
        embed_resource::compile("app.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
    // OUT_DIR is target/<profile>/build/<package>-<hash>/out; the exe is in target/<profile>.
    // ponytail: the workspace's exes share target/<profile>/assets, so a file two apps both have
    // must be the same file; give each app its own folder if that ever stops holding.
    let out = env::var("OUT_DIR").unwrap();
    let exe_folder = Path::new(&out).ancestors().nth(3).unwrap();
    copy_folder(Path::new("assets"), &exe_folder.join("assets"));
    // Linux has no exe resources: drawnui reads the window icon from icon.ico next to the exe.
    if target == "linux" {
        fs::copy("icon.ico", exe_folder.join("icon.ico")).unwrap();
    }
}

fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if source.is_dir() {
            copy_folder(&source, &target);
        } else {
            fs::copy(&source, &target).unwrap();
        }
    }
}
