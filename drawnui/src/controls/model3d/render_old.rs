// ---------------------------------------------------------------- the scene on the GPU

/// GL objects of the scene and the frame it is drawn into.
struct GlScene {
    epoch: u32,
    program: u32,
    uniforms: Uniforms,
    /// Per mesh, per primitive: vertex array, index count.
    meshes: Vec<Vec<(u32, i32)>>,
    textures: Vec<u32>,
    frame: u32,
    color: u32,
    depth: u32,
    size: ISize,
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
}

#[cfg(target_os = "emscripten")]
mod render {
    use super::*;
    use crate::gpu::{GlTarget, Gpu};
    use skia_safe::{ColorType, Image, gpu::SurfaceOrigin};

    const VS: &str = "#version 300 es
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_nrm;
layout(location=2) in vec2 a_uv;
layout(location=3) in vec4 a_joints;
layout(location=4) in vec4 a_weights;
uniform mat4 u_model;
uniform mat4 u_view_proj;
uniform mat4 u_joints[64];
uniform bool u_skinned;
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
}";

    const FS: &str = "#version 300 es
precision mediump float;
in vec3 v_nrm;
in vec2 v_uv;
uniform vec4 u_color;
uniform bool u_textured;
uniform sampler2D u_texture;
uniform vec3 u_light;
out vec4 o_color;
void main() {
    vec4 c = u_color;
    // glTF colors are linear; textures are sRGB: light in linear, show in sRGB.
    if (u_textured) { vec4 t = texture(u_texture, v_uv); c *= vec4(pow(t.rgb, vec3(2.2)), t.a); }
    if (c.a < 0.02) discard;
    vec3 n = normalize(v_nrm);
    if (!gl_FrontFacing) n = -n;
    float sun = max(dot(n, u_light), 0.0);
    float sky = 0.5 + 0.5 * n.y;
    vec3 lit = c.rgb * (0.30 + 0.25 * sky + 0.70 * sun);
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
    const DEPTH_TEST: u32 = 0x0B71;
    const CULL_FACE: u32 = 0x0B44;
    const BLEND: u32 = 0x0BE2;
    const SCISSOR_TEST: u32 = 0x0C11;
    const STENCIL_TEST: u32 = 0x0B90;
    const TRIANGLES: u32 = 0x0004;
    const LESS: u32 = 0x0201;
    const BACK: u32 = 0x0405;
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

    fn upload(scene: &Scene, epoch: u32) -> GlScene {
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
            };
            glPixelStorei(UNPACK_ALIGNMENT, 4);
            let textures = scene
                .images
                .iter()
                .map(|(rgba, w, h)| {
                    let mut t = 0;
                    glGenTextures(1, &mut t);
                    glBindTexture(TEXTURE_2D, t);
                    glTexImage2D(TEXTURE_2D, 0, RGBA8 as i32, *w as i32, *h as i32, 0, RGBA, UNSIGNED_BYTE, rgba.as_ptr());
                    glGenerateMipmap(TEXTURE_2D);
                    glTexParameteri(TEXTURE_2D, 0x2801, 0x2703); // MIN_FILTER LINEAR_MIPMAP_LINEAR
                    glTexParameteri(TEXTURE_2D, 0x2800, 0x2601); // MAG_FILTER LINEAR
                    t
                })
                .collect();
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
                            buffer(ELEMENT_ARRAY_BUFFER, bytes(&p.indices));
                            glBindVertexArray(0);
                            (vao, p.indices.len() as i32)
                        })
                        .collect()
                })
                .collect();
            GlScene { epoch, program, uniforms, meshes, textures, frame: 0, color: 0, depth: 0, size: ISize::new(0, 0) }
        }
    }

    fn ensure_frame(gl: &mut GlScene, size: ISize) {
        if gl.size == size && gl.frame != 0 {
            return;
        }
        // SAFETY: as `shader`.
        unsafe {
            if gl.frame == 0 {
                glGenFramebuffers(1, &mut gl.frame);
                glGenTextures(1, &mut gl.color);
                glGenRenderbuffers(1, &mut gl.depth);
            }
            glBindTexture(TEXTURE_2D, gl.color);
            glTexImage2D(TEXTURE_2D, 0, RGBA8 as i32, size.width, size.height, 0, RGBA, UNSIGNED_BYTE, std::ptr::null());
            glTexParameteri(TEXTURE_2D, 0x2801, 0x2601);
            glTexParameteri(TEXTURE_2D, 0x2800, 0x2601);
            glBindRenderbuffer(RENDERBUFFER, gl.depth);
            glRenderbufferStorage(RENDERBUFFER, DEPTH_COMPONENT24, size.width, size.height);
            glBindFramebuffer(FRAMEBUFFER, gl.frame);
            glFramebufferTexture2D(FRAMEBUFFER, COLOR_ATTACHMENT0, TEXTURE_2D, gl.color, 0);
            glFramebufferRenderbuffer(FRAMEBUFFER, DEPTH_ATTACHMENT, RENDERBUFFER, gl.depth);
        }
        gl.size = size;
    }

    /// Draws the scene in its pose into the frame texture and hands it to Skia as an image.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(slot: &mut Option<GlScene>, gpu: &mut Gpu, scene: &Scene, pose: &[Mat4], hidden: &[bool], overrides: &[MaterialOverride], view_proj: Mat4, size: ISize) -> Option<Image> {
        let gl = slot.get_or_insert_with(|| upload(scene, gpu.epoch()));
        ensure_frame(gl, size);
        let light = Vec3::new(0.4, 0.8, 0.6).normalize();
        // SAFETY: as `shader`. Skia's GL state is reset below.
        unsafe {
            glBindFramebuffer(FRAMEBUFFER, gl.frame);
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
            glUseProgram(gl.program);
            glUniformMatrix4fv(gl.uniforms.view_proj, 1, 0, view_proj.to_cols_array().as_ptr());
            glUniform3fv(gl.uniforms.light, 1, light.to_array().as_ptr());
            glUniform1i(gl.uniforms.texture, 0);
            glActiveTexture(TEXTURE0);
            let mut joints = vec![0f32; MAX_JOINTS * 16];
            // Opaque first, then blended, each in tree order.
            for pass_blend in [false, true] {
                if pass_blend {
                    glEnable(BLEND);
                    glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
                    glDepthMask(0);
                }
                for (i, node) in scene.nodes.iter().enumerate() {
                    let (Some(m), Some(vaos)) = (node.mesh, node.mesh.and_then(|m| gl.meshes.get(m))) else { continue };
                    if hidden[i] {
                        continue;
                    }
                    let skin = node.skin.and_then(|s| scene.skins.get(s));
                    let model = if skin.is_some() { Mat4::IDENTITY } else { pose[i] };
                    if let Some(skin) = skin {
                        for (j, (joint, ibm)) in skin.joints.iter().zip(&skin.inverse_bind).take(MAX_JOINTS).enumerate() {
                            joints[j * 16..j * 16 + 16].copy_from_slice(&(pose[*joint] * *ibm).to_cols_array());
                        }
                        glUniformMatrix4fv(gl.uniforms.joints, skin.joints.len().min(MAX_JOINTS) as i32, 0, joints.as_ptr());
                    }
                    glUniformMatrix4fv(gl.uniforms.model, 1, 0, model.to_cols_array().as_ptr());
                    for (p, (vao, count)) in scene.meshes[m].iter().zip(vaos) {
                        let material = p.material.and_then(|i| scene.materials.get(i));
                        let changed = p.material.and_then(|i| overrides.get(i)).copied().unwrap_or_default();
                        if changed.hidden || material.is_some_and(|m| m.blend) != pass_blend {
                            continue;
                        }
                        let color = changed.color.unwrap_or(material.map_or([0.8, 0.8, 0.8, 1.0], |m| m.color));
                        let texture = material.and_then(|m| m.texture).and_then(|t| gl.textures.get(t));
                        if material.is_some_and(|m| m.double_sided) { glDisable(CULL_FACE) } else { glEnable(CULL_FACE) }
                        glUniform4fv(gl.uniforms.color, 1, color.as_ptr());
                        glUniform1i(gl.uniforms.textured, texture.is_some() as i32);
                        glBindTexture(TEXTURE_2D, texture.copied().unwrap_or(0));
                        glUniform1i(gl.uniforms.skinned, (skin.is_some() && !p.joints.is_empty()) as i32);
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
            glBindVertexArray(0);
            glBindTexture(TEXTURE_2D, 0);
            glUseProgram(0);
            glDisable(DEPTH_TEST);
            glDisable(CULL_FACE);
            glDisable(BLEND);
            glDepthMask(1);
            glBindFramebuffer(FRAMEBUFFER, 0);
            glBindBuffer(ARRAY_BUFFER, 0);
            gpu.reset_gl_state();
            gpu.import_gl_texture(gl.color, GlTarget::Texture2D, RGBA8, size, SurfaceOrigin::BottomLeft, ColorType::RGBA8888)
        }
    }
}

#[cfg(not(target_os = "emscripten"))]
mod render {
    use super::*;
    use crate::gpu::Gpu;
    use skia_safe::Image;

    /// Not on this platform yet: nothing is drawn.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(_slot: &mut Option<GlScene>, _gpu: &mut Gpu, _scene: &Scene, _pose: &[Mat4], _hidden: &[bool], _overrides: &[MaterialOverride], _view_proj: Mat4, _size: ISize) -> Option<Image> {
        None
    }
}

