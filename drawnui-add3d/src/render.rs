//! The GPU side: one shader program and one copy of each model per GL context, shared by every
//! control; each control draws its objects in one pass into its own texture, which Skia shows.
//! WebGL2 (the browser) for now: elsewhere `draw` draws nothing.
#![cfg_attr(not(target_os = "emscripten"), allow(dead_code, unused_imports))]

use std::rc::Rc;

use glam::Mat4;
use drawnui::skia::{ISize, Image};

use super::scene::{MaterialOverride, Scene};
use drawnui::Gpu;

/// One object to draw: its scene, every node's world matrix (the object's place included), the
/// nodes hidden and the materials changed.
pub(super) struct Item<'a> {
    pub(super) scene: &'a Rc<Scene>,
    pub(super) pose: &'a [Mat4],
    pub(super) hidden: &'a [bool],
    pub(super) materials: &'a [MaterialOverride],
}

/// How a control draws its objects: the look, and an outline around every shape.
pub(super) struct Style {
    pub(super) look: super::Look,
    /// Outline width in pixels (0 = none).
    pub(super) outline: f32,
    /// Outline color, sRGB, premultiplied by the shader.
    pub(super) outline_color: [f32; 4],
    /// Smooth edges: drawn multisampled, then resolved into the control's texture.
    pub(super) antialias: bool,
}

/// Samples for smooth edges: 4, or the most the device has below that; 0 = none (fewer than 2).
pub(super) fn msaa_samples(max: i32) -> i32 {
    if max < 2 { 0 } else { max.min(4) }
}

/// A control's frame: the texture it draws into, with its depth buffer.
pub(super) struct Target {
    epoch: u32,
    frame: u32,
    color: u32,
    depth: u32,
    size: ISize,
    /// With `antialias`: the multisampled frame drawn into, its color and depth buffers, its
    /// samples and size; resolved into `color` after each draw.
    msaa: Option<Msaa>,
}

struct Msaa {
    frame: u32,
    color: u32,
    depth: u32,
    size: ISize,
}

#[cfg(not(target_os = "emscripten"))]
pub(super) fn draw(_target: &mut Option<Target>, _gpu: &mut Gpu, _items: &[Item], _style: &Style, _view_proj: Mat4, _size: ISize) -> Option<Image> {
    None
}

#[cfg(target_os = "emscripten")]
pub(super) use gl::draw;

#[cfg(target_os = "emscripten")]
mod gl {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Weak;

    use glam::Vec3;
    use drawnui::skia::{ColorType, gpu::SurfaceOrigin};

    use super::*;
    use drawnui::GlTarget;

    /// Joint matrices a skinned mesh may use (WebGL2 guarantees 256 vertex uniform vectors).
    const MAX_JOINTS: usize = 64;

    const VS: &str = "#version 300 es
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_nrm;
layout(location=2) in vec2 a_uv;
layout(location=3) in vec4 a_joints;
layout(location=4) in vec4 a_weights;
layout(location=5) in vec3 a_outline_nrm;
uniform mat4 u_model;
uniform mat4 u_view_proj;
uniform mat4 u_joints[64];
uniform bool u_skinned;
uniform float u_outline;
uniform vec2 u_viewport;
out vec3 v_nrm;
out vec2 v_uv;
void main() {
    mat4 skin = mat4(1.0);
    if (u_skinned) {
        skin = a_weights.x * u_joints[int(a_joints.x)] + a_weights.y * u_joints[int(a_joints.y)]
             + a_weights.z * u_joints[int(a_joints.z)] + a_weights.w * u_joints[int(a_joints.w)];
    }
    mat4 m = u_model * skin;
    v_nrm = mat3(m) * a_nrm;
    v_uv = a_uv;
    gl_Position = u_view_proj * m * vec4(a_pos, 1.0);
    if (u_outline > 0.0) {
        // The outline pass: the back faces pushed out along their smoothed normals (one per
        // position, so hard edges do not split), a fixed number of pixels on screen.
        vec4 n = u_view_proj * vec4(mat3(m) * a_outline_nrm, 0.0);
        vec2 dir = normalize(n.xy + vec2(1e-6));
        gl_Position.xy += dir * u_outline * 2.0 / u_viewport * gl_Position.w;
        // A little behind: where it meets a surface at the same depth, the surface wins.
        gl_Position.z += 0.002 * gl_Position.w;
    }
}";

    const FS: &str = "#version 300 es
precision mediump float;
in vec3 v_nrm;
in vec2 v_uv;
uniform vec4 u_color;
uniform bool u_textured;
uniform sampler2D u_texture;
uniform vec3 u_light;
uniform int u_look;
uniform bool u_outline_pass;
uniform vec4 u_outline_color;
out vec4 o_color;
void main() {
    if (u_outline_pass) { o_color = vec4(u_outline_color.rgb * u_outline_color.a, u_outline_color.a); return; }
    vec4 c = u_color;
    // glTF colors are linear; textures are sRGB: light in linear, show in sRGB.
    if (u_textured) { vec4 t = texture(u_texture, v_uv); c *= vec4(pow(t.rgb, vec3(2.2)), t.a); }
    if (c.a < 0.02) discard;
    vec3 n = normalize(v_nrm);
    if (!gl_FrontFacing) n = -n;
    float sun = max(dot(n, u_light), 0.0);
    float sky = 0.5 + 0.5 * n.y;
    vec3 lit;
    if (u_look == 2) lit = c.rgb;                                  // flat: the color as it is
    else if (u_look == 1) lit = c.rgb * (sun > 0.35 ? 1.0 : 0.62);  // toon: lit and shade, two tones
    else lit = c.rgb * (0.30 + 0.25 * sky + 0.70 * sun);
    o_color = vec4(pow(lit, vec3(1.0 / 2.2)) * c.a, c.a);
}";

    #[allow(non_snake_case)]
    unsafe extern "C" {
        fn glCreateShader(kind: u32) -> u32;
        fn glShaderSource(shader: u32, count: i32, strings: *const *const u8, lengths: *const i32);
        fn glCompileShader(shader: u32);
        fn glGetShaderiv(shader: u32, name: u32, out: *mut i32);
        fn glGetShaderInfoLog(shader: u32, size: i32, length: *mut i32, log: *mut u8);
        fn glCreateProgram() -> u32;
        fn glAttachShader(program: u32, shader: u32);
        fn glLinkProgram(program: u32);
        fn glGetProgramiv(program: u32, name: u32, out: *mut i32);
        fn glGetUniformLocation(program: u32, name: *const u8) -> i32;
        fn glUseProgram(program: u32);
        fn glGenVertexArrays(n: i32, out: *mut u32);
        fn glBindVertexArray(vao: u32);
        fn glGenBuffers(n: i32, out: *mut u32);
        fn glBindBuffer(target: u32, buffer: u32);
        fn glBufferData(target: u32, size: isize, data: *const u8, usage: u32);
        fn glEnableVertexAttribArray(index: u32);
        fn glDisableVertexAttribArray(index: u32);
        fn glVertexAttribPointer(index: u32, size: i32, kind: u32, normalized: u8, stride: i32, offset: usize);
        fn glVertexAttrib4f(index: u32, x: f32, y: f32, z: f32, w: f32);
        fn glGenTextures(n: i32, out: *mut u32);
        fn glBindTexture(target: u32, texture: u32);
        fn glTexImage2D(target: u32, level: i32, internal: i32, w: i32, h: i32, border: i32, format: u32, kind: u32, data: *const u8);
        fn glTexParameteri(target: u32, name: u32, value: i32);
        fn glGenerateMipmap(target: u32);
        fn glActiveTexture(unit: u32);
        fn glGenFramebuffers(n: i32, out: *mut u32);
        fn glBindFramebuffer(target: u32, frame: u32);
        fn glFramebufferTexture2D(target: u32, attachment: u32, tex_target: u32, texture: u32, level: i32);
        fn glGenRenderbuffers(n: i32, out: *mut u32);
        fn glBindRenderbuffer(target: u32, buffer: u32);
        fn glRenderbufferStorage(target: u32, format: u32, w: i32, h: i32);
        fn glRenderbufferStorageMultisample(target: u32, samples: i32, format: u32, w: i32, h: i32);
        fn glBlitFramebuffer(x0: i32, y0: i32, x1: i32, y1: i32, dx0: i32, dy0: i32, dx1: i32, dy1: i32, mask: u32, filter: u32);
        fn glDeleteFramebuffers(n: i32, frames: *const u32);
        fn glDeleteRenderbuffers(n: i32, buffers: *const u32);
        fn glGetIntegerv(name: u32, out: *mut i32);
        fn glFramebufferRenderbuffer(target: u32, attachment: u32, rb_target: u32, buffer: u32);
        fn glViewport(x: i32, y: i32, w: i32, h: i32);
        fn glClearColor(r: f32, g: f32, b: f32, a: f32);
        fn glClearDepthf(d: f32);
        fn glClear(mask: u32);
        fn glEnable(cap: u32);
        fn glDisable(cap: u32);
        fn glDepthMask(on: u8);
        fn glColorMask(r: u8, g: u8, b: u8, a: u8);
        fn glDepthFunc(func: u32);
        fn glCullFace(face: u32);
        fn glFrontFace(mode: u32);
        fn glBlendFunc(src: u32, dst: u32);
        fn glUniformMatrix4fv(location: i32, count: i32, transpose: u8, value: *const f32);
        fn glUniform4fv(location: i32, count: i32, value: *const f32);
        fn glUniform3fv(location: i32, count: i32, value: *const f32);
        fn glUniform1i(location: i32, value: i32);
        fn glUniform1f(location: i32, value: f32);
        fn glUniform2f(location: i32, x: f32, y: f32);
        fn glDrawElements(mode: u32, count: i32, kind: u32, offset: usize);
        fn glPixelStorei(name: u32, value: i32);
    }

    const ARRAY_BUFFER: u32 = 0x8892;
    const ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
    const STATIC_DRAW: u32 = 0x88E4;
    const FLOAT: u32 = 0x1406;
    const UNSIGNED_BYTE: u32 = 0x1401;
    const UNSIGNED_INT: u32 = 0x1405;
    const TEXTURE_2D: u32 = 0x0DE1;
    const RGBA: u32 = 0x1908;
    const RGBA8: u32 = 0x8058;
    const FRAMEBUFFER: u32 = 0x8D40;
    const RENDERBUFFER: u32 = 0x8D41;
    const COLOR_ATTACHMENT0: u32 = 0x8CE0;
    const DEPTH_ATTACHMENT: u32 = 0x8D00;
    const DEPTH_COMPONENT24: u32 = 0x81A6;
    const READ_FRAMEBUFFER: u32 = 0x8CA8;
    const DRAW_FRAMEBUFFER: u32 = 0x8CA9;
    const MAX_SAMPLES: u32 = 0x8D57;
    const COLOR_BUFFER_BIT: u32 = 0x4000;
    const NEAREST: u32 = 0x2600;
    const DEPTH_TEST: u32 = 0x0B71;
    const CULL_FACE: u32 = 0x0B44;
    const BLEND: u32 = 0x0BE2;
    const SCISSOR_TEST: u32 = 0x0C11;
    const STENCIL_TEST: u32 = 0x0B90;
    const TRIANGLES: u32 = 0x0004;
    const LESS: u32 = 0x0201;
    const BACK: u32 = 0x0405;
    const FRONT: u32 = 0x0404;
    const CCW: u32 = 0x0901;
    const ONE: u32 = 1;
    const ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
    const TEXTURE0: u32 = 0x84C0;
    const UNPACK_ALIGNMENT: u32 = 0x0CF5;

    fn shader(kind: u32, source: &str) -> u32 {
        // SAFETY: GL calls on the current WebGL2 context, with valid pointers.
        unsafe {
            let s = glCreateShader(kind);
            let (ptr, len) = (source.as_ptr(), source.len() as i32);
            glShaderSource(s, 1, &ptr, &len);
            glCompileShader(s);
            let mut ok = 0;
            glGetShaderiv(s, 0x8B81, &mut ok);
            if ok == 0 {
                let mut log = vec![0u8; 1024];
                let mut n = 0;
                glGetShaderInfoLog(s, 1024, &mut n, log.as_mut_ptr());
                eprintln!("SkiaModel3D shader: {}", String::from_utf8_lossy(&log[..n.max(0) as usize]));
            }
            s
        }
    }

    fn buffer(target: u32, bytes: &[u8]) -> u32 {
        // SAFETY: as `shader`.
        unsafe {
            let mut b = 0;
            glGenBuffers(1, &mut b);
            glBindBuffer(target, b);
            glBufferData(target, bytes.len() as isize, bytes.as_ptr(), STATIC_DRAW);
            b
        }
    }

    fn bytes<T>(v: &[T]) -> &[u8] {
        // SAFETY: plain f32 / u32 arrays.
        unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
    }

    fn attribute(index: u32, data: &[u8], size: i32) {
        if data.is_empty() {
            return;
        }
        buffer(ARRAY_BUFFER, data);
        // SAFETY: as `shader`; the buffer just bound.
        unsafe {
            glEnableVertexAttribArray(index);
            glVertexAttribPointer(index, size, FLOAT, 0, 0, 0);
        }
    }

    /// One normal per position, the average of the normals there: an outline pushed along them
    /// stays closed at hard edges (a coin's rim, a box's corner).
    fn outline_normals(p: &super::super::scene::Primitive) -> Vec<[f32; 3]> {
        if !p.outline_normals.is_empty() {
            return p.outline_normals.clone();
        }
        let key = |v: &[f32; 3]| v.map(|c| (c * 10_000.0).round() as i32);
        let mut sums: HashMap<[i32; 3], Vec3> = HashMap::new();
        for (v, n) in p.positions.iter().zip(&p.normals) {
            *sums.entry(key(v)).or_insert(Vec3::ZERO) += Vec3::from(*n);
        }
        p.positions.iter().map(|v| sums[&key(v)].normalize_or_zero().to_array()).collect()
    }

    struct Uniforms {
        model: i32,
        view_proj: i32,
        joints: i32,
        skinned: i32,
        color: i32,
        textured: i32,
        texture: i32,
        light: i32,
        look: i32,
        outline: i32,
        viewport: i32,
        outline_pass: i32,
        outline_color: i32,
    }

    /// One model on the GPU: per mesh, per primitive its vertex array and index count; its textures.
    struct Model {
        meshes: Vec<Vec<(u32, i32)>>,
        textures: Vec<u32>,
    }

    /// The GL objects of one context: the program and the models uploaded to it.
    struct Context {
        epoch: u32,
        program: u32,
        uniforms: Uniforms,
        // ponytail: a model whose scene is gone keeps its GL objects until the context goes; delete
        // them when apps load and drop many different models.
        models: HashMap<usize, (Weak<Scene>, Rc<Model>)>,
        /// Images put on materials from code (`set_material_image`), by their pixels' address.
        images: HashMap<usize, (Weak<(Vec<u8>, u32, u32)>, u32)>,
    }

    thread_local! {
        static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
    }

    fn program() -> (u32, Uniforms) {
        // SAFETY: as `shader`.
        unsafe {
            let program = glCreateProgram();
            glAttachShader(program, shader(0x8B31, VS));
            glAttachShader(program, shader(0x8B30, FS));
            glLinkProgram(program);
            let mut ok = 0;
            glGetProgramiv(program, 0x8B82, &mut ok);
            if ok == 0 {
                eprintln!("SkiaModel3D: the shader program did not link");
            }
            let loc = |name: &str| glGetUniformLocation(program, format!("{name}\0").as_ptr());
            let uniforms = Uniforms {
                model: loc("u_model"),
                view_proj: loc("u_view_proj"),
                joints: loc("u_joints"),
                skinned: loc("u_skinned"),
                color: loc("u_color"),
                textured: loc("u_textured"),
                texture: loc("u_texture"),
                light: loc("u_light"),
                look: loc("u_look"),
                outline: loc("u_outline"),
                viewport: loc("u_viewport"),
                outline_pass: loc("u_outline_pass"),
                outline_color: loc("u_outline_color"),
            };
            (program, uniforms)
        }
    }

    /// One RGBA image as a mipmapped texture.
    fn texture((rgba, w, h): &(Vec<u8>, u32, u32)) -> u32 {
        // SAFETY: as `shader`.
        unsafe {
            glPixelStorei(UNPACK_ALIGNMENT, 4);
            let mut t = 0;
            glGenTextures(1, &mut t);
            glBindTexture(TEXTURE_2D, t);
            glTexImage2D(TEXTURE_2D, 0, RGBA8 as i32, *w as i32, *h as i32, 0, RGBA, UNSIGNED_BYTE, rgba.as_ptr());
            glGenerateMipmap(TEXTURE_2D);
            glTexParameteri(TEXTURE_2D, 0x2801, 0x2703); // MIN_FILTER LINEAR_MIPMAP_LINEAR
            glTexParameteri(TEXTURE_2D, 0x2800, 0x2601); // MAG_FILTER LINEAR
            t
        }
    }

    /// The texture of an image put on a material, uploaded the first time it is drawn.
    // ponytail: like models, a texture whose image is gone is deleted only when the context goes.
    fn image_texture(context: &mut Context, image: &super::super::scene::Texture3D) -> u32 {
        let key = Rc::as_ptr(&image.0) as usize;
        if let Some((weak, t)) = context.images.get(&key)
            && weak.upgrade().is_some_and(|i| Rc::ptr_eq(&i, &image.0))
        {
            return *t;
        }
        let t = texture(&image.0);
        context.images.retain(|_, (weak, _)| weak.strong_count() > 0);
        context.images.insert(key, (Rc::downgrade(&image.0), t));
        t
    }

    fn upload(scene: &Scene) -> Model {
        // SAFETY: as `shader`.
        unsafe {
            let textures = scene.images.iter().map(texture).collect();
            let meshes = scene
                .meshes
                .iter()
                .map(|primitives| {
                    primitives
                        .iter()
                        .map(|p| {
                            let mut vao = 0;
                            glGenVertexArrays(1, &mut vao);
                            glBindVertexArray(vao);
                            attribute(0, bytes(&p.positions), 3);
                            attribute(1, bytes(&p.normals), 3);
                            attribute(2, bytes(&p.uvs), 2);
                            attribute(3, bytes(&p.joints), 4);
                            attribute(4, bytes(&p.weights), 4);
                            attribute(5, bytes(&outline_normals(p)), 3);
                            buffer(ELEMENT_ARRAY_BUFFER, bytes(&p.indices));
                            glBindVertexArray(0);
                            (vao, p.indices.len() as i32)
                        })
                        .collect()
                })
                .collect();
            Model { meshes, textures }
        }
    }

    /// The scene's model on this context, uploaded the first time any control draws it.
    fn model(context: &mut Context, scene: &Rc<Scene>) -> Rc<Model> {
        let key = Rc::as_ptr(scene) as usize;
        if let Some((weak, model)) = context.models.get(&key)
            && weak.upgrade().is_some_and(|s| Rc::ptr_eq(&s, scene))
        {
            return model.clone();
        }
        let model = Rc::new(upload(scene));
        context.models.retain(|_, (weak, _)| weak.strong_count() > 0);
        context.models.insert(key, (Rc::downgrade(scene), model.clone()));
        model
    }

    fn ensure_target(slot: &mut Option<Target>, epoch: u32, size: ISize) -> (u32, u32) {
        if slot.as_ref().is_some_and(|t| t.epoch != epoch) {
            *slot = None;
        }
        let target = slot.get_or_insert_with(|| {
            let (mut frame, mut color, mut depth) = (0, 0, 0);
            // SAFETY: as `shader`.
            unsafe {
                glGenFramebuffers(1, &mut frame);
                glGenTextures(1, &mut color);
                glGenRenderbuffers(1, &mut depth);
            }
            Target { epoch, frame, color, depth, size: ISize::new(0, 0), msaa: None }
        });
        if target.size != size {
            // SAFETY: as `shader`.
            unsafe {
                glBindTexture(TEXTURE_2D, target.color);
                glTexImage2D(TEXTURE_2D, 0, RGBA8 as i32, size.width, size.height, 0, RGBA, UNSIGNED_BYTE, std::ptr::null());
                glTexParameteri(TEXTURE_2D, 0x2801, 0x2601);
                glTexParameteri(TEXTURE_2D, 0x2800, 0x2601);
                glBindRenderbuffer(RENDERBUFFER, target.depth);
                glRenderbufferStorage(RENDERBUFFER, DEPTH_COMPONENT24, size.width, size.height);
                glBindFramebuffer(FRAMEBUFFER, target.frame);
                glFramebufferTexture2D(FRAMEBUFFER, COLOR_ATTACHMENT0, TEXTURE_2D, target.color, 0);
                glFramebufferRenderbuffer(FRAMEBUFFER, DEPTH_ATTACHMENT, RENDERBUFFER, target.depth);
            }
            target.size = size;
        }
        (target.frame, target.color)
    }

    /// The multisampled frame to draw into when `antialias` is on (made or resized here), `None`
    /// when off or the device has no multisampling; turning it off frees its buffers.
    fn ensure_msaa(target: &mut Option<Target>, on: bool, size: ISize) -> Option<u32> {
        let target = target.as_mut()?;
        let samples = if on {
            let mut max = 0;
            // SAFETY: as `shader`.
            unsafe { glGetIntegerv(MAX_SAMPLES, &mut max) };
            super::msaa_samples(max)
        } else {
            0
        };
        if samples == 0 {
            if let Some(m) = target.msaa.take() {
                // SAFETY: as `shader`; these objects belong to this target only.
                unsafe {
                    glDeleteFramebuffers(1, &m.frame);
                    glDeleteRenderbuffers(2, [m.color, m.depth].as_ptr());
                }
            }
            return None;
        }
        let msaa = target.msaa.get_or_insert_with(|| {
            let (mut frame, mut buffers) = (0, [0u32; 2]);
            // SAFETY: as `shader`.
            unsafe {
                glGenFramebuffers(1, &mut frame);
                glGenRenderbuffers(2, buffers.as_mut_ptr());
            }
            Msaa { frame, color: buffers[0], depth: buffers[1], size: ISize::new(0, 0) }
        });
        if msaa.size != size {
            // SAFETY: as `shader`.
            unsafe {
                glBindRenderbuffer(RENDERBUFFER, msaa.color);
                glRenderbufferStorageMultisample(RENDERBUFFER, samples, RGBA8, size.width, size.height);
                glBindRenderbuffer(RENDERBUFFER, msaa.depth);
                glRenderbufferStorageMultisample(RENDERBUFFER, samples, DEPTH_COMPONENT24, size.width, size.height);
                glBindFramebuffer(FRAMEBUFFER, msaa.frame);
                glFramebufferRenderbuffer(FRAMEBUFFER, COLOR_ATTACHMENT0, RENDERBUFFER, msaa.color);
                glFramebufferRenderbuffer(FRAMEBUFFER, DEPTH_ATTACHMENT, RENDERBUFFER, msaa.depth);
            }
            msaa.size = size;
        }
        Some(msaa.frame)
    }

    /// Draws the objects into the control's texture in one pass and hands it to Skia as an image.
    pub(in super::super) fn draw(target: &mut Option<Target>, gpu: &mut Gpu, items: &[Item], style: &Style, view_proj: Mat4, size: ISize) -> Option<Image> {
        let epoch = gpu.epoch();
        let models: Vec<Rc<Model>> = CONTEXT.with_borrow_mut(|slot| {
            if slot.as_ref().is_some_and(|c| c.epoch != epoch) {
                *slot = None;
            }
            let context = slot.get_or_insert_with(|| {
                let (program, uniforms) = program();
                Context { epoch, program, uniforms, models: HashMap::new(), images: HashMap::new() }
            });
            for image in items.iter().flat_map(|item| item.materials.iter().filter_map(|m| m.image.as_ref())) {
                image_texture(context, image);
            }
            items.iter().map(|item| model(context, item.scene)).collect()
        });
        let (resolved, color) = ensure_target(target, epoch, size);
        let msaa = ensure_msaa(target, style.antialias, size);
        let frame = msaa.unwrap_or(resolved);
        let light = Vec3::new(0.4, 0.8, 0.6).normalize();
        CONTEXT.with_borrow(|context| {
            let context = context.as_ref().expect("made above");
            let u = &context.uniforms;
            // SAFETY: as `shader`. Skia's GL state is reset below.
            unsafe {
                glBindFramebuffer(FRAMEBUFFER, frame);
                glViewport(0, 0, size.width, size.height);
                glDisable(SCISSOR_TEST);
                glDisable(STENCIL_TEST);
                glDisable(BLEND);
                glColorMask(1, 1, 1, 1);
                glDepthMask(1);
                glClearColor(0.0, 0.0, 0.0, 0.0);
                glClearDepthf(1.0);
                glClear(0x4000 | 0x0100);
                glEnable(DEPTH_TEST);
                glDepthFunc(LESS);
                glFrontFace(CCW);
                glCullFace(BACK);
                glUseProgram(context.program);
                glUniformMatrix4fv(u.view_proj, 1, 0, view_proj.to_cols_array().as_ptr());
                glUniform3fv(u.light, 1, light.to_array().as_ptr());
                glUniform1i(u.texture, 0);
                glUniform1i(u.look, style.look as i32);
                glUniform2f(u.viewport, size.width as f32, size.height as f32);
                glUniform4fv(u.outline_color, 1, style.outline_color.as_ptr());
                glActiveTexture(TEXTURE0);
                let mut joints = vec![0f32; MAX_JOINTS * 16];
                // The outline first (back faces pushed out), then opaque, then blended, each in tree order.
                let passes: &[(bool, bool)] = if style.outline > 0.0 { &[(true, false), (false, false), (false, true)] } else { &[(false, false), (false, true)] };
                for &(outline_pass, pass_blend) in passes {
                    glUniform1i(u.outline_pass, outline_pass as i32);
                    glUniform1f(u.outline, if outline_pass { style.outline } else { 0.0 });
                    glCullFace(if outline_pass { FRONT } else { BACK });
                    if pass_blend {
                        glEnable(BLEND);
                        glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
                        glDepthMask(0);
                    }
                    for (item, gl) in items.iter().zip(&models) {
                        let scene = item.scene;
                        for (i, node) in scene.nodes.iter().enumerate() {
                            let (Some(m), Some(vaos)) = (node.mesh, node.mesh.and_then(|m| gl.meshes.get(m))) else { continue };
                            if item.hidden.get(i).copied().unwrap_or(false) {
                                continue;
                            }
                            let skin = node.skin.and_then(|s| scene.skins.get(s));
                            // A skinned mesh is placed by its joints only (glTF ignores its node's
                            // transform); the object's place is in the joints' world matrices.
                            let model = if skin.is_some() { Mat4::IDENTITY } else { item.pose[i] };
                            if let Some(skin) = skin {
                                for (j, (joint, ibm)) in skin.joints.iter().zip(&skin.inverse_bind).take(MAX_JOINTS).enumerate() {
                                    joints[j * 16..j * 16 + 16].copy_from_slice(&(item.pose[*joint] * *ibm).to_cols_array());
                                }
                                glUniformMatrix4fv(u.joints, skin.joints.len().min(MAX_JOINTS) as i32, 0, joints.as_ptr());
                            }
                            glUniformMatrix4fv(u.model, 1, 0, model.to_cols_array().as_ptr());
                            for (p, (vao, count)) in scene.meshes[m].iter().zip(vaos) {
                                let material = p.material.and_then(|i| scene.materials.get(i));
                                let changed = p.material.and_then(|i| item.materials.get(i));
                                if changed.is_some_and(|c| c.hidden) || material.is_some_and(|m| m.blend) != pass_blend {
                                    continue;
                                }
                                let color = changed.and_then(|c| c.color).unwrap_or(material.map_or([0.8, 0.8, 0.8, 1.0], |m| m.color));
                                let texture = match changed.and_then(|c| c.image.as_ref()) {
                                    Some(image) => context.images.get(&(Rc::as_ptr(&image.0) as usize)).map(|e| &e.1),
                                    None => material.and_then(|m| m.texture).and_then(|t| gl.textures.get(t)),
                                };
                                if material.is_some_and(|m| m.double_sided) && !outline_pass { glDisable(CULL_FACE) } else { glEnable(CULL_FACE) }
                                glUniform4fv(u.color, 1, color.as_ptr());
                                glUniform1i(u.textured, texture.is_some() as i32);
                                glBindTexture(TEXTURE_2D, texture.copied().unwrap_or(0));
                                glUniform1i(u.skinned, (skin.is_some() && !p.joints.is_empty()) as i32);
                                glBindVertexArray(*vao);
                                if p.joints.is_empty() {
                                    glDisableVertexAttribArray(3);
                                    glDisableVertexAttribArray(4);
                                    glVertexAttrib4f(3, 0.0, 0.0, 0.0, 0.0);
                                    glVertexAttrib4f(4, 1.0, 0.0, 0.0, 0.0);
                                }
                                glDrawElements(TRIANGLES, *count, UNSIGNED_INT, 0);
                            }
                        }
                    }
                }
                glBindVertexArray(0);
                glBindTexture(TEXTURE_2D, 0);
                glUseProgram(0);
                glDisable(DEPTH_TEST);
                glDisable(CULL_FACE);
                glDisable(BLEND);
                glDepthMask(1);
                if let Some(msaa) = msaa {
                    glBindFramebuffer(READ_FRAMEBUFFER, msaa);
                    glBindFramebuffer(DRAW_FRAMEBUFFER, resolved);
                    glBlitFramebuffer(0, 0, size.width, size.height, 0, 0, size.width, size.height, COLOR_BUFFER_BIT, NEAREST);
                }
                glBindFramebuffer(FRAMEBUFFER, 0);
                glBindBuffer(ARRAY_BUFFER, 0);
            }
        });
        gpu.reset_gl_state();
        // SAFETY: the texture lives with the control's target; the frame drawing it ends before the
        // next draw into it (WebGL runs one queue in order).
        unsafe { gpu.import_gl_texture(color, GlTarget::Texture2D, RGBA8, size, SurfaceOrigin::BottomLeft, ColorType::RGBA8888) }
    }
}
