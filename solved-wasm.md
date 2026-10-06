# Solved: Rust + Skia on wasm

Problems DrawnUi.Rust hit in the browser build (skia-safe, `wasm32-unknown-emscripten`) and how
they were fixed. Short, for anyone porting a Rust + Skia library to the web. Add every new one.

1. **Target.** skia-safe links only for `wasm32-unknown-emscripten`, not
   `wasm32-unknown-unknown`. That rules out winit, wasm-bindgen and web-sys. We wrote a small JS
   host (WebGL2 context, requestAnimationFrame, input, fetch), and the Rust side exports plain
   `extern "C"` functions. emsdk lives in its own folder and is activated per process, so PATH
   stays untouched.

2. **Skia has to be compiled for wasm.** rust-skia's prebuilt wasm binaries use the old exception
   ABI and do not link with Rust 1.93 or newer (rust-skia issue 1287). Set `FORCE_SKIA_BUILD=1`
   for the wasm build only. The first build takes about 6 min; later builds are Rust-only. Any
   change to rustflags or link args compiles Skia again, so settle the flags early. You need LLVM,
   Python and Ninja.

3. **Extra Skia features** (SkMesh in our case, Skottie too) are missing from prebuilt binaries
   too. Our patched rust-skia, wired in through `[patch.crates-io]`, therefore means a source
   build on desktop as well. To stop every target dir and every parallel agent from compiling
   Skia (we once had 120 clang processes), we build it once with `BUILD_ARTIFACTSTAGINGDIRECTORY`
   and pack the result as a rust-skia binaries archive. `.cargo/config.toml` `[env]` then points
   every build at it: `SKIA_BINARIES_URL=file://...` and `FORCE_SKIA_BINARIES_DOWNLOAD=1`. A
   fresh target dir now builds in 30 s with zero clang. Rebuild the archive after any rust-skia
   commit or feature change. See `solved-win.md`.

4. **emsdk 6 on Windows ships `.exe` tools, no `.bat` files**, while Skia's Windows toolchain and
   the cc crate default to `.bat`. Name `emcc.exe` / `em++.exe` / `emar.exe` explicitly
   (`SKIA_GN_ARGS` cc/cxx/ar, `CC_/CXX_/AR_wasm32_unknown_emscripten`) and compile Skia with
   `-fwasm-exceptions` to match what rustc links with. Skia's build calls `python3`; where that
   is only the Microsoft Store stub, a `python3.bat` shim forwards to `python`. `dev/build.ps1`
   does all of it and restores the environment after.

5. **THE BIG ONE: the wasm stack.** Emscripten's default stack is 64 KB, and nothing checks for
   overflow by default. Recursive layout and paint on a deep page needed about 90 KB, so the
   stack silently overwrote memory. Symptoms: `memory allocation of N bytes failed`,
   `memory access out of bounds`, `RuntimeError: unreachable`, and errors that move to pages
   opened later. The desktop build was fine. Prove it by linking with
   `-sSTACK_OVERFLOW_CHECK=2`, which aborts with `stack overflow (Attempt to set SP to ...)`.
   Permanent fix: `-sSTACK_SIZE=1048576` together with `-sSTACK_OVERFLOW_CHECK=1`. If you get
   memory-corruption errors only on wasm, check the stack first.

6. **Link args we use** (linker `em++`): `-sMAX_WEBGL_VERSION=2 -sMODULARIZE=1
   -sEXPORT_NAME=<name> -sEXPORTED_RUNTIME_METHODS=GL,HEAPU8,UTF8ToString -sALLOW_MEMORY_GROWTH=1
   -sERROR_ON_UNDEFINED_SYMBOLS=0 -sSTACK_SIZE=1048576 -sSTACK_OVERFLOW_CHECK=1`.

7. **No system fonts, no file system.** JS fetches fonts and images as bytes and passes them to
   wasm. Load large fallback fonts (emoji) lazily so they never delay the first frame.

8. **A Windows resource in the wasm link.** `embed-resource` in an app's `build.rs` decides by the
   machine the build script runs on (Windows), not by the target, so the wasm build linked the
   app's icon resource and failed: `wasm-ld: error: unknown file type: ...\out\app.lib`. Check
   `CARGO_CFG_TARGET_OS` in `build.rs` and embed only for `windows`; any host-side build step
   (resources, asset copies) needs the same check.

9. **Stale JavaScript after a publish.** The host (Cloudflare zone) let browsers keep `.js` for 4
   hours while the page and the `.wasm` were revalidated on every load: a returning visitor could
   run the new wasm with the old emscripten glue and host script. `dev/build.ps1` now stamps the
   addresses with content hashes: the glue's `locateFile("<app>.wasm")` becomes
   `"<app>.wasm?v=<hash>"`, the page loads `<app>.js?v=<hash>`, `drawnui_host.js?v=<hash>` and
   `favicon.ico?v=<hash>`, and it hands the host a map of every asset's hash
   (`window.duiAssetVersions`): images, fonts, SVG, Lottie and shaders are asked for as
   `assets/...?v=<hash>`. Unchanged files keep their address and their cache.

10. **A lost WebGL context.** Browsers drop the WebGL context on a GPU reset, under memory
    pressure or for a background tab on mobile; without handling, the canvas stays dead. The host
    calls `preventDefault()` on `webglcontextlost` (else the context never comes back), stops
    frames, and on `webglcontextrestored` makes the context current again in emscripten
    (`GL.registerContext` + `makeContextCurrent`), builds a new Skia context (`dui_gpu_restored`)
    and surface; the engine drops what lived on the old context and makes it again. Abandon the
    old Skia context with `abandon()`, not `release_resources_and_abandon()`: the latter calls GL
    to delete objects of the dead context ("delete: object does not belong to this context").
    Test with `gl.getExtension('WEBGL_lose_context').loseContext()` / `restoreContext()`.

11. **Skia for wasm from an archive, not a source build per machine.** `dev/skia-binaries.ps1 -Web`
    compiles wasm Skia once (with wasm exceptions) and exports it as a rust-skia binaries archive;
    `dev/build.ps1` and other machines take it (`SKIA_BINARIES_URL` + `FORCE_SKIA_BINARIES_DOWNLOAD`,
    no `FORCE_SKIA_BUILD`): a fresh target dir builds in about 70 s. With forced download rust-skia
    takes the key's commit hash from `git rev-parse` in the skia-bindings folder: the rust-skia
    checkout must be a git repo at the archive's commit, not a `git archive` export, and not a
    folder inside another repo. Our rust-skia checkout is shallow, so `git bundle` fails ("remote did
    not send all necessary objects"); `git clone --depth 1 -b skmesh file:///<path>/rust-skia` works
    (HEAD is the archive's commit) and can be packed with its `.git` for another machine.

    Faster iteration builds: link with `-C link-arg=-O1` (emcc takes the last -O) to skip the
    wasm-opt passes. Measured by the DrawFiddle builder: a snippet build 2-3 s instead of 12 s; frame
    CPU scrolling 1500 rows the same (median 0.5 ms, p95 0.8-1.5 ms); wasm 4.99 MB instead of
    4.26 MB raw, 1.29 MB instead of 1.27 MB brotli. Keep the full -O link for published builds.

12. **Payload.** textlayout (HarfBuzz + ICU), svg and skottie roughly double the wasm size. Our
   full demo is 6.3 MB raw, 1.8 MB brotli.
