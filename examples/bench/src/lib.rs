//! Phase 0 scene: the same drawing code on the desktop and in the browser.
//! Shapes-only mode is the G0 gate workload (N draw calls per frame, nothing else).
//! Mesh mode draws N animated SkMesh waves (custom vertex and fragment programs).

use std::sync::atomic::{AtomicI32, Ordering};

use drawnui::{
    App, Frame, Host, PointerKind,
    skia::{
        Blender, Canvas, Color, Data, Font, FontMgr, IRect, Image, Mesh, MeshSpecification, Paint, Picture,
        PictureRecorder, RRect, Rect, RuntimeEffect, Typeface,
        mesh::{Attribute, Mode, Varying, VertexBuffer, attribute, varying},
        meshes,
    },
};

static SHAPES: AtomicI32 = AtomicI32::new(2000);
/// 0 the whole scene, 1 shapes only (G0), 2 meshes.
static MODE: AtomicI32 = AtomicI32::new(0);
const SHAPES_ONLY: i32 = 1;
const MESHES: i32 = 2;

/// The scene's size and mode (`run` and the web page's `bench_config` set it).
pub fn configure(shapes: i32, mode: i32) {
    SHAPES.store(shapes, Ordering::Relaxed);
    MODE.store(mode, Ordering::Relaxed);
}

// The animated wave of the DrawnUi.Net SKMesh probe (BlazorSandbox, SkMeshProbe.razor): a
// triangle strip whose vertex program moves every vertex by a time uniform.
const WAVE_COLUMNS: usize = 24;
const WAVE_VS: &str = "
uniform float uTime;
uniform float uAmp;
uniform float uFreq;
Varyings main(const Attributes a) {
    Varyings v;
    float dy = sin(a.position.x * uFreq + uTime) * uAmp;
    v.position = a.position + float2(0.0, dy);
    v.uv = a.uv;
    return v;
}";
const WAVE_FS: &str = "
float2 main(const Varyings v, out half4 color) {
    float band = abs(sin(v.uv.x * 12.0));
    color = half4(band, 0.4 + 0.5 * v.uv.y, 1.0 - band, 1.0);
    return v.position;
}";

/// What every wave shares: the compiled programs and one strip the size of a cell.
struct Waves {
    spec: MeshSpecification,
    vertices: VertexBuffer,
    cell: (f32, f32),
}

impl Waves {
    fn new(cell: (f32, f32)) -> Self {
        let attributes =
            [Attribute::new(attribute::Type::Float2, 0, "position"), Attribute::new(attribute::Type::Float2, 8, "uv")];
        let varyings = [Varying::new(varying::Type::Float2, "uv")];
        let spec = MeshSpecification::make(&attributes, 16, &varyings, WAVE_VS, WAVE_FS).expect("mesh SkSL");
        // The strip leaves a quarter of the cell free above and below: the wave moves into it.
        let (top, bottom) = (cell.1 * 0.25, cell.1 * 0.75);
        let mut strip = Vec::with_capacity((WAVE_COLUMNS + 1) * 8);
        for i in 0..=WAVE_COLUMNS {
            let u = i as f32 / WAVE_COLUMNS as f32;
            strip.extend([u * cell.0, top, u, 0.0, u * cell.0, bottom, u, 1.0]);
        }
        let bytes: Vec<u8> = strip.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let vertices = meshes::make_vertex_buffer(&bytes).expect("vertex buffer");
        Self { spec, vertices, cell }
    }
}

const FONT: u32 = 1;

const SKSL: &str = "
uniform float2 size;
uniform float time;
half4 main(float2 p) {
    float2 uv = p / size;
    float v = 0.5 + 0.5 * sin(10.0 * uv.x + time) * cos(8.0 * uv.y - time);
    return half4(uv.x * v, uv.y * v, v, 1.0);
}";

#[derive(Default)]
struct Bench {
    typeface: Option<Typeface>,
    cache: Option<(Image, u32)>,
    card: Option<Picture>,
    effect: Option<RuntimeEffect>,
    waves: Option<Waves>,
    pointer: (f32, f32),
    taps: u32,
    frames: u32,
    fps: f32,
    fps_frames: u32,
    fps_since_ms: f64,
}

impl Bench {
    fn card() -> Option<Picture> {
        let mut recorder = PictureRecorder::new();
        let canvas = recorder.begin_recording(Rect::from_wh(200.0, 120.0), false);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from_argb(255, 40, 44, 60));
        canvas.draw_rrect(RRect::new_rect_xy(Rect::from_wh(200.0, 120.0), 12.0, 12.0), &paint);
        for i in 0..6u8 {
            paint.set_color(Color::from_argb(255, 80 + i * 25, 160, 255 - i * 30));
            canvas.draw_circle((24.0 + i as f32 * 30.0, 60.0), 12.0, &paint);
        }
        recorder.finish_recording_as_picture(None)
    }

    /// Image cache: drawn once into an offscreen GPU surface, blitted every frame.
    fn cache(frame: &mut Frame) -> Option<Image> {
        let mut offscreen = frame.gpu.offscreen(256, 256)?;
        let canvas = offscreen.canvas();
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        for i in 0..32u8 {
            paint.set_color(Color::from_argb(200, 255 - i * 6, 90 + i * 4, 60 + i * 5));
            canvas.draw_circle((128.0, 128.0), 126.0 - i as f32 * 3.8, &paint);
        }
        frame.gpu.snapshot(&mut offscreen, None)
    }

    /// Payload measurement only: pulls in paragraph layout (ICU, HarfBuzz) and the SVG module.
    #[cfg(feature = "full")]
    fn full_features(canvas: &Canvas, typeface: &Typeface) {
        use drawnui::skia::{
            svg,
            textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle, TypefaceFontProvider},
        };
        let mut provider = TypefaceFontProvider::new();
        provider.register_typeface(typeface.clone(), Some("Bench"));
        let mut fonts = FontCollection::new();
        fonts.set_asset_font_manager(Some(provider.into()));
        let mut text = TextStyle::new();
        text.set_font_families(&["Bench"]);
        text.set_font_size(18.0);
        text.set_color(Color::WHITE);
        let mut style = ParagraphStyle::new();
        style.set_text_style(&text);
        let mut builder = ParagraphBuilder::new(&style, fonts);
        builder.add_text("Paragraph layout through ICU and HarfBuzz wraps this sentence across lines.");
        let mut paragraph = builder.build();
        paragraph.layout(260.0);
        paragraph.paint(canvas, (340.0, 170.0));
        let source = br#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><circle cx="32" cy="32" r="28" fill="tomato"/></svg>"#;
        if let Ok(dom) = svg::Dom::from_bytes(source, FontMgr::new()) {
            canvas.save();
            canvas.translate((620.0, 170.0));
            dom.render(canvas);
            canvas.restore();
        }
    }

    /// `count` waves in a grid over the canvas, each with its own phase: one mesh and one draw
    /// call per wave, the uniforms new every frame.
    fn draw_waves(&mut self, canvas: &Canvas, width: f32, height: f32, t: f32, count: usize) {
        let columns = ((count as f32 * width / height).sqrt().ceil() as usize).max(1);
        let cell = (width / columns as f32, height / count.div_ceil(columns) as f32);
        if self.waves.as_ref().is_none_or(|w| w.cell != cell) {
            self.waves = Some(Waves::new(cell));
        }
        let Some(waves) = &self.waves else { return };
        // The mesh color is multiplied by the paint color.
        let mut paint = Paint::default();
        paint.set_color(Color::WHITE);
        for i in 0..count {
            let uniforms = [t * 3.0 + i as f32 * 0.4, cell.1 * 0.2, std::f32::consts::TAU * 1.5 / cell.0];
            let uniforms: Vec<u8> = uniforms.iter().flat_map(|v| v.to_ne_bytes()).collect();
            let (spec, vertices, vertex_count) = (waves.spec.clone(), waves.vertices.clone(), (WAVE_COLUMNS + 1) * 2);
            let bounds = Rect::from_wh(cell.0, cell.1);
            let mesh = Mesh::make(spec, Mode::TriangleStrip, vertices, vertex_count, 0, Data::new_copy(&uniforms), &[], bounds);
            let Ok(mesh) = mesh else { continue };
            canvas.save();
            canvas.translate(((i % columns) as f32 * cell.0, (i / columns) as f32 * cell.1));
            canvas.draw_mesh(&mesh, None::<Blender>, &paint);
            canvas.restore();
        }
    }

    /// Verification: BENCH_SHOT=<file.png> saves frame 120.
    #[cfg(not(target_os = "emscripten"))]
    fn shot(&self, frame: &mut Frame) {
        if self.frames == 120
            && let Ok(path) = std::env::var("BENCH_SHOT")
        {
            let mut bitmap = drawnui::skia::Bitmap::new();
            bitmap.alloc_n32_pixels((frame.width as i32, frame.height as i32), false);
            frame.surface.read_pixels_to_bitmap(&bitmap, (0, 0));
            let png = bitmap.as_image().encode(None, drawnui::skia::EncodedImageFormat::PNG, None).expect("png");
            std::fs::write(path, png.as_bytes()).expect("shot");
        }
    }
    #[cfg(target_os = "emscripten")]
    fn shot(&self, _frame: &mut Frame) {}

    /// Every benchmark shows its frame rate on screen, top right.
    fn draw_fps(&self, canvas: &Canvas, width: f32, scale: f32) {
        let Some(typeface) = &self.typeface else { return };
        let font = Font::from_typeface(typeface, 20.0 * scale);
        let text = format!("FPS {:.0}", self.fps);
        let (text_width, _) = font.measure_str(&text, None);
        let pill = Rect::from_xywh(width - text_width - 32.0 * scale, 8.0 * scale, text_width + 24.0 * scale, 32.0 * scale);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from_argb(220, 0, 0, 0));
        canvas.draw_rrect(RRect::new_rect_xy(pill, 8.0 * scale, 8.0 * scale), &paint);
        paint.set_color(Color::from_argb(255, 80, 255, 120));
        canvas.draw_str(text, (pill.left + 12.0 * scale, pill.top + 23.0 * scale), &font, &paint);
    }
}

impl App for Bench {
    fn init(&mut self, host: &mut Host) {
        host.fetch(FONT, "assets/OpenSans-Regular.ttf");
    }

    fn asset(&mut self, id: u32, bytes: Vec<u8>) {
        if id == FONT {
            self.typeface = FontMgr::new().new_from_data(Data::new_copy(&bytes), None);
        }
    }

    fn pointer(&mut self, kind: PointerKind, x: f32, y: f32, _time_ms: f64) {
        self.pointer = (x, y);
        if kind == PointerKind::Up {
            self.taps += 1;
        }
    }

    fn frame(&mut self, frame: &mut Frame) -> bool {
        self.frames += 1;
        self.fps_frames += 1;
        let elapsed = frame.time_ms - self.fps_since_ms;
        if elapsed >= 500.0 {
            self.fps = self.fps_frames as f32 * 1000.0 / elapsed as f32;
            self.fps_frames = 0;
            self.fps_since_ms = frame.time_ms;
        }
        let (w, h, scale) = (frame.width, frame.height, frame.scale);
        let t = (frame.time_ms * 0.001) as f32;
        let mode = MODE.load(Ordering::Relaxed);
        let shapes_only = mode == SHAPES_ONLY;

        if mode == MESHES {
            let canvas = frame.surface.canvas();
            canvas.clear(Color::from_argb(255, 18, 18, 24));
            self.draw_waves(canvas, w, h, t, SHAPES.load(Ordering::Relaxed).max(1) as usize);
            self.draw_fps(canvas, w, scale);
            self.shot(frame);
            return true;
        }

        if !shapes_only && self.cache.as_ref().is_none_or(|(_, epoch)| *epoch != frame.gpu.epoch()) {
            self.cache = Self::cache(frame).map(|image| (image, frame.gpu.epoch()));
        }

        let canvas = frame.surface.canvas();
        canvas.clear(Color::from_argb(255, 18, 18, 24));

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        for i in 0..SHAPES.load(Ordering::Relaxed) {
            let fi = i as f32;
            let x = (0.5 + 0.45 * (t * 0.7 + fi * 0.37).sin()) * w;
            let y = (0.5 + 0.45 * (t * 0.9 + fi * 0.53).cos()) * h;
            paint.set_color(Color::from_argb(255, (i * 37 % 256) as u8, (i * 73 % 256) as u8, (i * 151 % 256) as u8));
            canvas.draw_rrect(RRect::new_rect_xy(Rect::from_xywh(x, y, 24.0, 16.0), 4.0, 4.0), &paint);
        }
        if shapes_only {
            self.draw_fps(canvas, w, scale);
            return true;
        }

        // SkSL runtime effect.
        let effect = self.effect.get_or_insert_with(|| RuntimeEffect::make_for_shader(SKSL, None).expect("SkSL"));
        let uniforms: Vec<u8> = [300.0f32, 200.0, t].iter().flat_map(|v| v.to_ne_bytes()).collect();
        if let Some(shader) = effect.make_shader(Data::new_copy(&uniforms), &[], None) {
            let mut shaded = Paint::default();
            shaded.set_shader(shader);
            canvas.save();
            canvas.translate((20.0, 20.0));
            canvas.draw_rect(Rect::from_wh(300.0, 200.0), &shaded);
            canvas.restore();
        }

        // Picture replay.
        let card = self.card.get_or_insert_with(|| Self::card().expect("picture"));
        for i in 0..4 {
            canvas.save();
            canvas.translate((340.0 + i as f32 * 210.0, 20.0 + 20.0 * (t + i as f32).sin()));
            canvas.draw_picture(&*card, None, None);
            canvas.restore();
        }

        // Image cache blit.
        if let Some((image, _)) = &self.cache {
            canvas.draw_image(image, (20.0 + 100.0 * (1.0 + t.sin()), 240.0), None);
        }

        // Text from the fetched font.
        if let Some(typeface) = &self.typeface {
            let font = Font::from_typeface(typeface, 22.0 * scale);
            paint.set_color(Color::WHITE);
            let text = format!("DrawnUi.Rust phase 0 | frame {} | taps {}", self.frames, self.taps);
            canvas.draw_str(text, (20.0, h - 24.0 * scale), &font, &paint);
            #[cfg(feature = "full")]
            Self::full_features(canvas, typeface);
        }

        // Pointer.
        paint.set_color(Color::from_argb(200, 255, 220, 0));
        canvas.draw_circle(self.pointer, 18.0 * scale, &paint);

        // Snapshot of what is on the surface so far, drawn back scaled (the backdrop case).
        let (px, py) = (self.pointer.0 as i32, self.pointer.1 as i32);
        let region = IRect::from_xywh(px - 60, py - 60, 120, 120);
        if let Some(snapshot) = frame.gpu.snapshot(frame.surface, Some(region)) {
            let dst = Rect::from_xywh(w - 260.0, h - 260.0, 240.0, 240.0);
            frame.surface.canvas().draw_image_rect(snapshot, None, dst, &Paint::default());
        }
        self.draw_fps(frame.surface.canvas(), w, scale);
        self.shot(frame);
        true
    }
}

/// Runs the scene. `args`: none (2,000 shapes, the whole scene), `<n>`, `<n> g0` (the G0
/// workload: shapes only), `<n> mesh` (n SkMesh waves).
pub fn run(args: impl IntoIterator<Item = String>) {
    let mut args = args.into_iter();
    if let Some(shapes) = args.next().and_then(|a| a.parse().ok()) {
        // `bench 20000 g0`: the G0 workload; `bench 200 mesh`: 200 SkMesh waves.
        let mode = match args.next().as_deref() {
            Some("g0") => SHAPES_ONLY,
            Some("mesh") => MESHES,
            _ => 0,
        };
        configure(shapes, mode);
    }
    drawnui::run("DrawnUi.Rust bench", || Box::new(Bench::default()));
}
