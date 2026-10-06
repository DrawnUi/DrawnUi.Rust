// The bench window: `bench`, `bench <n>`, `bench <n> g0`, `bench <n> mesh` (the scene is the
// library, so the Android app can run it too).

/// Web: called by the page before the first frame.
#[unsafe(no_mangle)]
pub extern "C" fn bench_config(shapes: i32, mode: i32) {
    bench::configure(shapes, mode);
}

fn main() {
    bench::run(std::env::args().skip(1));
}
