# Hot reload for DrawnUI Rust apps

What we learned (2026-10-01) when we planned Rust snippets for the DrawnUI Fiddle. There, "hot
reload" means: you stop typing, the code compiles again and the canvas redraws. C# and TSX do this
in the browser in under a second. Rust cannot.

## Why Rust is different

- **No compiler in the browser.** There is no rustc that runs in a web page and links Skia. Every
  change has to be compiled on a server, so every run costs a network round trip and a build.
- **The build links the whole engine.** A web build is one wasm file with drawnui, Skia and the
  Rust standard library inside (about 3.8 MB for the bench example). Changing one line still means
  linking all of it again.

## Measured rebuild times

Intel Core Ultra 9 185H (16 cores), Windows, `wasm32-unknown-emscripten`, `bench` example (316
lines). drawnui and Skia were already compiled; only the app crate was rebuilt and linked.

| Profile | Rebuild after a change |
|---|---|
| `release` (opt-level 3, thin LTO) | 16 s |
| no LTO, opt-level 2, incremental, 16 codegen units | 10 s |

The first build of a new profile compiles Skia for wasm again: 6 min 08 s. A 2 vCPU server will
be slower than these numbers; not measured.

## Three levels of hot reload

**1. Full rebuild on every pause.** Works today. About 10 s per change, too slow to feel like hot
reload. A lighter link step (no wasm optimizer pass) could bring it down; not measured.

**2. Live values.** When an edit changes only values (colors, numbers, texts), send the new values
to the running app instead of compiling. The build rewrites each literal into a table lookup; the
editor compares the new code with the code it built, and when only literals differ it sends the
new table. Instant for value edits; any other edit falls back to a full rebuild. Jetpack Compose
("Live Literals") and Dioxus (markup hot reload) do the same. Strings and colors are simple;
numbers need care with Rust type inference (`16` can be `i32` or `f32`), so prove it with a spike
first. Only the editor build has the table; a published build compiles the values normally.

**3. Hot patching.** Keep the engine loaded, compile only the changed functions into a small
patch, and redirect calls to the new versions through a jump table. Dioxus does this with
Subsecond (about 130 ms per patch on an Apple M4, their number). Subsecond's web support is built
on wasm-bindgen, i.e. `wasm32-unknown-unknown`; we use `wasm32-unknown-emscripten`. The same idea
on emscripten would use its dynamic linking (side modules that share the main module's memory and
function table, loaded with `dlopen`); building a Rust side module for emscripten has open issues
(rust-lang/rust#80775), and the main module has to export every symbol, which makes the engine
download bigger. Subsecond's limits would apply too: a changed struct layout (a new field in
`App`) needs a restart, statics survive patches, thread-locals reset. Weeks of research, high risk.
Expected in the Fiddle: network round trip + incremental rustc + small link, maybe 1 to 3 s; not
measured.

## Why not build our code for `wasm32-unknown-unknown` and keep Skia on emscripten?

- **Two modules, two memories.** Each wasm module has its own memory. skia-safe passes pointers to
  paths, paints, text and pixels, and a pointer means nothing in the other module. Every Skia call
  would copy its data across and Skia objects would become handles: the CanvasKit model, which
  DrawnUi.React already uses. Our G0 measurement: Rust calling Skia directly uses 1.3 to 1.65 times
  less CPU than CanvasKit driven from JS. This split gives that away on every frame, and needs new
  handle-based bindings instead of skia-safe.
- **One module, one memory.** The C ABI is no longer the problem: since 2025 Rust's
  `wasm32-unknown-unknown` uses the standard wasm C ABI. The problem is that Skia needs a C
  library, the C++ standard library and WebGL functions, and on `unknown-unknown` nothing provides
  them; emscripten does (with its JavaScript GL layer). rust-skia tried in issue 1078: it fails on
  `'cstddef' file not found`; a WASI build compiles, but wasm-bindgen does not support WASI. Making
  it work means rebuilding emscripten's runtime by hand. Nobody has done it publicly.

Hot patching on emscripten (level 3 above) keeps Skia as it is and costs nothing per frame; it is
the only one of these worth a research spike.

## Sources

- Subsecond: https://docs.rs/subsecond/latest/subsecond/
- Dioxus 0.7 release: https://dioxuslabs.com/blog/release-070/
- Emscripten dynamic linking: https://emscripten.org/docs/compiling/Dynamic-Linking.html
- rust-lang/rust#80775 (cdylib for wasm32-unknown-emscripten): https://github.com/rust-lang/rust/issues/80775
- rust-skia#1078 (revisiting wasm32-unknown-unknown): https://github.com/rust-skia/rust-skia/issues/1078
- C ABI changes for wasm32-unknown-unknown: https://blog.rust-lang.org/2025/04/04/c-abi-changes-for-wasm32-unknown-unknown/
