//! Metal (macOS, iOS): the same drawing as the WebGL2 path, on the device and command queue Skia
//! draws with. The shaders take their vertices from buffers by index, so a primitive without
//! texture coordinates or a skin gets default ones uploaded instead. Our command buffer is
//! committed while the frame is painted, before Skia's work of that frame on the same queue, so
//! Skia reads the finished texture.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::{Rc, Weak};
use std::sync::{Mutex, Once};
use std::thread::JoinHandle;

use drawnui::skia::{ColorType, gpu::SurfaceOrigin};
use glam::{Mat4, Vec3};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBlitCommandEncoder, MTLBuffer, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCompareFunction, MTLCreateSystemDefaultDevice, MTLCullMode,
    MTLDepthStencilDescriptor, MTLDepthStencilState, MTLDevice, MTLIndexType, MTLLibrary, MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRegion, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLResourceOptions, MTLSamplerAddressMode, MTLSamplerDescriptor, MTLSamplerMinMagFilter, MTLSamplerMipFilter,
    MTLSamplerState, MTLSize, MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureType, MTLTextureUsage, MTLWinding,
};

use super::super::scene::{Scene, Texture3D};
use super::{Item, Style};
use drawnui::Gpu;
use drawnui::skia::{ISize, Image};

/// Joint matrices a skinned mesh may use, as in the WebGL2 path.
const MAX_JOINTS: usize = 64;

/// The WebGL2 shaders in Metal's language. The vertex stage ends by moving GL's depth (-1..1)
/// into Metal's (0..1), so the projections stay the same for both.
const SHADERS: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct Frame {
    float4x4 view_proj;
    float4 eye;
    float4 light;
    float4 outline_color;
    float2 viewport;
    float outline;
    int look;
    int outline_pass;
};

struct Object {
    float4x4 model;
    float4 color;
    float rough;
    int skinned;
    int textured;
};

struct VOut {
    float4 position [[position]];
    float3 nrm;
    float3 pos;
    float2 uv;
};

vertex VOut vs(uint vid [[vertex_id]],
               const device packed_float3* positions [[buffer(0)]],
               const device packed_float3* normals [[buffer(1)]],
               const device float2* uvs [[buffer(2)]],
               const device float4* joints [[buffer(3)]],
               const device float4* weights [[buffer(4)]],
               const device packed_float3* outline_normals [[buffer(5)]],
               constant Frame& f [[buffer(6)]],
               constant Object& o [[buffer(7)]],
               constant float4x4* joint_matrices [[buffer(8)]]) {
    float4x4 skin = float4x4(float4(1, 0, 0, 0), float4(0, 1, 0, 0), float4(0, 0, 1, 0), float4(0, 0, 0, 1));
    if (o.skinned != 0) {
        float4 j = joints[vid];
        float4 w = weights[vid];
        skin = w.x * joint_matrices[int(j.x)] + w.y * joint_matrices[int(j.y)]
             + w.z * joint_matrices[int(j.z)] + w.w * joint_matrices[int(j.w)];
    }
    float4x4 m = o.model * skin;
    float3x3 m3 = float3x3(m[0].xyz, m[1].xyz, m[2].xyz);
    VOut out;
    out.nrm = m3 * float3(normals[vid]);
    out.uv = uvs[vid];
    float4 world = m * float4(float3(positions[vid]), 1.0);
    out.pos = world.xyz;
    float4 p = f.view_proj * world;
    if (f.outline > 0.0) {
        // The outline pass: the back faces pushed out along their smoothed normals, a fixed
        // number of pixels on screen, a little behind.
        float4 n = f.view_proj * float4(m3 * float3(outline_normals[vid]), 0.0);
        float2 dir = normalize(n.xy + float2(1e-6));
        p.xy += dir * f.outline * 2.0 / f.viewport * p.w;
        p.z += 0.002 * p.w;
    }
    p.z = 0.5 * (p.z + p.w);
    out.position = p;
    return out;
}

fragment float4 fs(VOut in [[stage_in]],
                   bool front [[front_facing]],
                   constant Frame& f [[buffer(0)]],
                   constant Object& o [[buffer(1)]],
                   texture2d<float> image [[texture(0)]],
                   sampler smp [[sampler(0)]]) {
    if (f.outline_pass != 0) { return float4(f.outline_color.rgb * f.outline_color.a, f.outline_color.a); }
    float4 c = o.color;
    // glTF colors are linear; textures are sRGB: light in linear, show in sRGB.
    if (o.textured != 0) { float4 t = image.sample(smp, in.uv); c *= float4(pow(t.rgb, float3(2.2)), t.a); }
    if (c.a < 0.02) discard_fragment();
    float3 n = normalize(in.nrm);
    if (!front) n = -n;
    float3 light = f.light.xyz;
    float sun = max(dot(n, light), 0.0);
    float sky = 0.5 + 0.5 * n.y;
    float3 lit;
    if (f.look == 2) {
        lit = c.rgb;
    } else if (f.look == 1) {
        lit = c.rgb * (sun > 0.35 ? 1.0 : 0.62);
    } else {
        float fill = max(dot(n, normalize(float3(-light.x, 0.3, -light.z))), 0.0);
        lit = c.rgb * (0.24 + 0.20 * sky + 0.68 * sun + 0.22 * fill);
        float gloss = 1.0 - clamp(o.rough, 0.0, 1.0);
        if (gloss > 0.01 && sun > 0.0) {
            float3 v = f.eye.w > 0.5 ? normalize(f.eye.xyz - in.pos) : normalize(f.eye.xyz);
            float s = pow(max(dot(n, normalize(light + v)), 0.0), mix(6.0, 90.0, gloss * gloss));
            lit += float3(0.55 * gloss * gloss * s);
        }
    }
    return float4(pow(lit, float3(1.0 / 2.2)) * c.a, c.a);
}
"#;

/// `Frame` of the shaders: what every object of a pass shares.
#[repr(C)]
#[derive(Clone, Copy)]
struct Frame {
    view_proj: [f32; 16],
    eye: [f32; 4],
    light: [f32; 4],
    outline_color: [f32; 4],
    viewport: [f32; 2],
    outline: f32,
    look: i32,
    outline_pass: i32,
    _pad: [i32; 3],
}

/// `Object` of the shaders: one primitive's place and material.
#[repr(C)]
#[derive(Clone, Copy)]
struct Object {
    model: [f32; 16],
    color: [f32; 4],
    rough: f32,
    skinned: i32,
    textured: i32,
    _pad: i32,
}

type Device = ProtocolObject<dyn MTLDevice>;
type Texture = Retained<ProtocolObject<dyn MTLTexture>>;
type Buffer = Retained<ProtocolObject<dyn MTLBuffer>>;
type Pipeline = Retained<ProtocolObject<dyn MTLRenderPipelineState>>;

/// The compiled shaders: the pipelines for opaque and blended passes, single sampled and with
/// `samples` (0 = the device has no multisampling).
struct Program {
    device: Retained<Device>,
    samples: usize,
    /// [opaque, opaque multisampled, blended, blended multisampled].
    pipelines: [Option<Pipeline>; 4],
}

fn pipeline_index(blend: bool, msaa: bool) -> usize {
    blend as usize * 2 + msaa as usize
}

impl Program {
    fn new(device: Retained<Device>) -> Option<Self> {
        let library = device
            .newLibraryWithSource_options_error(&NSString::from_str(SHADERS), None)
            .map_err(|e| eprintln!("SkiaModel3D shader: {}", e.localizedDescription()))
            .ok()?;
        let vs = library.newFunctionWithName(&NSString::from_str("vs"))?;
        let fs = library.newFunctionWithName(&NSString::from_str("fs"))?;
        let max = [4, 2].into_iter().find(|n| device.supportsTextureSampleCount(*n)).unwrap_or(1);
        let samples = super::msaa_samples(max as i32) as usize;
        let make = |blend: bool, count: usize| -> Option<Pipeline> {
            let d = MTLRenderPipelineDescriptor::new();
            d.setVertexFunction(Some(&vs));
            d.setFragmentFunction(Some(&fs));
            // SAFETY: index 0 always exists.
            let color = unsafe { d.colorAttachments().objectAtIndexedSubscript(0) };
            color.setPixelFormat(MTLPixelFormat::RGBA8Unorm);
            if blend {
                color.setBlendingEnabled(true);
                color.setSourceRGBBlendFactor(MTLBlendFactor::One);
                color.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
                color.setSourceAlphaBlendFactor(MTLBlendFactor::One);
                color.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
            }
            d.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float);
            d.setRasterSampleCount(count);
            device.newRenderPipelineStateWithDescriptor_error(&d).map_err(|e| eprintln!("SkiaModel3D pipeline: {}", e.localizedDescription())).ok()
        };
        let multi = |blend| if samples > 1 { make(blend, samples) } else { None };
        let pipelines = [make(false, 1), multi(false), make(true, 1), multi(true)];
        pipelines[0].as_ref()?;
        Some(Self { device, samples, pipelines })
    }
}

static START: Once = Once::new();
static STARTED: Mutex<Option<JoinHandle<Option<Program>>>> = Mutex::new(None);

/// Compiles the shaders and makes the pipelines on a thread of their own, on the system's default
/// device (the one Skia draws with), while the app lays out: the first 3D frame only collects
/// them. Called when a 3D control gets its properties; once per process.
pub(in super::super) fn start_program() {
    START.call_once(|| {
        let job = std::thread::spawn(|| MTLCreateSystemDefaultDevice().and_then(Program::new));
        if let Ok(mut slot) = STARTED.lock() {
            *slot = Some(job);
        }
    });
}

/// The program for `device`: the one started early when it was made on it, else made now.
fn program(device: &Retained<Device>) -> Option<Program> {
    let started = STARTED.lock().ok().and_then(|mut slot| slot.take()).and_then(|job| job.join().ok()).flatten();
    match started {
        Some(p) if Retained::as_ptr(&p.device) == Retained::as_ptr(device) => Some(p),
        _ => Program::new(device.clone()),
    }
}

/// One primitive on the GPU: its vertex buffers (positions, normals, texture coordinates, joints,
/// weights, outline normals), its indices and their count.
struct Primitive {
    vertices: [Buffer; 6],
    indices: Buffer,
    count: usize,
}

/// One model on the GPU: per mesh its primitives; its textures.
struct Model {
    meshes: Vec<Vec<Option<Primitive>>>,
    textures: Vec<Texture>,
}

/// The Metal objects of one context: the device, the queue, the program, the states and the models
/// uploaded to it.
struct Context {
    epoch: u32,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    program: Program,
    /// Depth test with writes (opaque and outline) and without (blended).
    depth: [Retained<ProtocolObject<dyn MTLDepthStencilState>>; 2],
    sampler: Retained<ProtocolObject<dyn MTLSamplerState>>,
    /// A white pixel, bound where nothing is textured.
    blank: Texture,
    // ponytail: a model whose scene is gone keeps its buffers until the context goes, as on WebGL.
    models: HashMap<usize, (Weak<Scene>, Rc<Model>)>,
    /// Images put on materials from code (`set_material_image`), by their pixels' address.
    images: HashMap<usize, (Weak<(Vec<u8>, u32, u32)>, Texture)>,
}

thread_local! {
    static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
}

/// A control's frame: the texture it draws into and its depth buffer, and with `antialias` the
/// multisampled color and depth drawn into, resolved into `color`.
pub(in super::super) struct Target {
    epoch: u32,
    size: ISize,
    color: Texture,
    depth: Texture,
    msaa: Option<(Texture, Texture)>,
}

fn bytes<T>(v: &[T]) -> &[u8] {
    // SAFETY: plain f32 / u32 arrays.
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
}

fn buffer(device: &Device, data: &[u8]) -> Option<Buffer> {
    let ptr = NonNull::new(data.as_ptr() as *mut c_void)?;
    // SAFETY: `data` is valid for its length; the buffer copies it.
    unsafe { device.newBufferWithBytes_length_options(ptr, data.len(), MTLResourceOptions::StorageModeShared) }
}

/// Memory the CPU writes a texture's pixels into: shared on iOS, managed on macOS (every Mac).
fn cpu_storage() -> MTLStorageMode {
    if cfg!(target_os = "ios") { MTLStorageMode::Shared } else { MTLStorageMode::Managed }
}

/// One RGBA image as a mipmapped texture; the mipmaps are made on `queue` before any later work.
fn texture(device: &Device, queue: &ProtocolObject<dyn MTLCommandQueue>, (rgba, w, h): &(Vec<u8>, u32, u32)) -> Option<Texture> {
    let (w, h) = ((*w).max(1) as usize, (*h).max(1) as usize);
    if rgba.len() < w * h * 4 {
        return None;
    }
    // SAFETY: a valid format and a size above zero.
    let d = unsafe { MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(MTLPixelFormat::RGBA8Unorm, w, h, true) };
    d.setUsage(MTLTextureUsage::ShaderRead);
    d.setStorageMode(cpu_storage());
    let t = device.newTextureWithDescriptor(&d)?;
    let region = MTLRegion { origin: MTLOrigin { x: 0, y: 0, z: 0 }, size: MTLSize { width: w, height: h, depth: 1 } };
    // SAFETY: `rgba` holds w x h RGBA pixels, rows of 4 x w bytes.
    unsafe { t.replaceRegion_mipmapLevel_withBytes_bytesPerRow(region, 0, NonNull::new(rgba.as_ptr() as *mut c_void)?, 4 * w) };
    if let Some(commands) = queue.commandBuffer()
        && let Some(blit) = commands.blitCommandEncoder()
    {
        blit.generateMipmapsForTexture(&t);
        blit.endEncoding();
        commands.commit();
    }
    Some(t)
}

/// A texture the control draws into, `samples` > 1 multisampled; GPU memory only.
fn render_texture(device: &Device, format: MTLPixelFormat, size: ISize, samples: usize, read: bool) -> Option<Texture> {
    // SAFETY: a valid format and a size above zero.
    let d = unsafe { MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(format, size.width as usize, size.height as usize, false) };
    if samples > 1 {
        d.setTextureType(MTLTextureType::Type2DMultisample);
        // SAFETY: a count the device supports.
        unsafe { d.setSampleCount(samples) };
    }
    d.setUsage(if read { MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead } else { MTLTextureUsage::RenderTarget });
    d.setStorageMode(MTLStorageMode::Private);
    device.newTextureWithDescriptor(&d)
}

impl Context {
    fn new(gpu: &Gpu, epoch: u32) -> Option<Self> {
        let (device, queue) = gpu.metal_handles()?;
        // SAFETY: the handles are the live `id<MTLDevice>` and `id<MTLCommandQueue>` Skia draws with;
        // retaining keeps them for this context.
        let device: Retained<Device> = unsafe { Retained::retain(device as *mut Device) }?;
        let queue: Retained<ProtocolObject<dyn MTLCommandQueue>> = unsafe { Retained::retain(queue as *mut ProtocolObject<dyn MTLCommandQueue>) }?;
        let program = program(&device)?;
        let depth_state = |write: bool| {
            let d = MTLDepthStencilDescriptor::new();
            d.setDepthCompareFunction(MTLCompareFunction::Less);
            d.setDepthWriteEnabled(write);
            device.newDepthStencilStateWithDescriptor(&d)
        };
        let depth = [depth_state(true)?, depth_state(false)?];
        let s = MTLSamplerDescriptor::new();
        s.setMinFilter(MTLSamplerMinMagFilter::Linear);
        s.setMagFilter(MTLSamplerMinMagFilter::Linear);
        s.setMipFilter(MTLSamplerMipFilter::Linear);
        s.setSAddressMode(MTLSamplerAddressMode::Repeat);
        s.setTAddressMode(MTLSamplerAddressMode::Repeat);
        // Text and fine lines on a surface at an angle (a tilted card) stay sharp.
        s.setMaxAnisotropy(8);
        let sampler = device.newSamplerStateWithDescriptor(&s)?;
        let blank = texture(&device, &queue, &(vec![255; 4], 1, 1))?;
        Some(Self { epoch, queue, program, depth, sampler, blank, models: HashMap::new(), images: HashMap::new() })
    }

    fn device(&self) -> &Device {
        &self.program.device
    }

    /// The scene's model on this context, uploaded the first time any control draws it.
    fn model(&mut self, scene: &Rc<Scene>) -> Rc<Model> {
        let key = Rc::as_ptr(scene) as usize;
        if let Some((weak, model)) = self.models.get(&key)
            && weak.upgrade().is_some_and(|s| Rc::ptr_eq(&s, scene))
        {
            return model.clone();
        }
        let model = Rc::new(self.upload(scene));
        self.models.retain(|_, (weak, _)| weak.strong_count() > 0);
        self.models.insert(key, (Rc::downgrade(scene), model.clone()));
        model
    }

    fn upload(&self, scene: &Scene) -> Model {
        let device = self.device();
        let textures = scene.images.iter().map(|i| texture(device, &self.queue, i).unwrap_or_else(|| self.blank.clone())).collect();
        let meshes = scene
            .meshes
            .iter()
            .map(|primitives| {
                primitives
                    .iter()
                    .map(|p| {
                        let n = p.positions.len();
                        if n == 0 || p.indices.is_empty() {
                            return None;
                        }
                        // Missing attributes get the WebGL2 defaults: no normal, texture point 0,
                        // no joints, all weight on the first.
                        let or = |v: &[u8], default: Vec<u8>| if v.is_empty() { default } else { v.to_vec() };
                        let normals = or(bytes(&p.normals), bytes(&vec![[0f32; 3]; n]).to_vec());
                        let uvs = or(bytes(&p.uvs), bytes(&vec![[0f32; 2]; n]).to_vec());
                        let joints = or(bytes(&p.joints), bytes(&vec![[0f32; 4]; n]).to_vec());
                        let weights = or(bytes(&p.weights), bytes(&vec![[1f32, 0.0, 0.0, 0.0]; n]).to_vec());
                        let outline = super::outline_normals(p);
                        let vertices = [
                            buffer(device, bytes(&p.positions))?,
                            buffer(device, &normals)?,
                            buffer(device, &uvs)?,
                            buffer(device, &joints)?,
                            buffer(device, &weights)?,
                            buffer(device, bytes(&outline))?,
                        ];
                        Some(Primitive { vertices, indices: buffer(device, bytes(&p.indices))?, count: p.indices.len() })
                    })
                    .collect()
            })
            .collect();
        Model { meshes, textures }
    }

    /// The texture of an image put on a material, uploaded the first time it is drawn.
    fn image(&mut self, image: &Texture3D) {
        let key = Rc::as_ptr(&image.0) as usize;
        if let Some((weak, _)) = self.images.get(&key)
            && weak.upgrade().is_some_and(|i| Rc::ptr_eq(&i, &image.0))
        {
            return;
        }
        let Some(t) = texture(self.device(), &self.queue, &image.0) else { return };
        self.images.retain(|_, (weak, _)| weak.strong_count() > 0);
        self.images.insert(key, (Rc::downgrade(&image.0), t));
    }

    /// The control's frame at `size` for this context, made again when either changed; the
    /// multisampled part with `antialias` on a device that has it, dropped when it is off.
    fn target<'a>(&self, slot: &'a mut Option<Target>, size: ISize, antialias: bool) -> Option<&'a Target> {
        let samples = if antialias { self.program.samples } else { 0 };
        let fits = |t: &Target| t.epoch == self.epoch && t.size == size && t.msaa.is_some() == (samples > 1);
        if !slot.as_ref().is_some_and(fits) {
            let device = self.device();
            let msaa = match samples > 1 {
                true => Some((
                    render_texture(device, MTLPixelFormat::RGBA8Unorm, size, samples, false)?,
                    render_texture(device, MTLPixelFormat::Depth32Float, size, samples, false)?,
                )),
                false => None,
            };
            *slot = Some(Target {
                epoch: self.epoch,
                size,
                color: render_texture(device, MTLPixelFormat::RGBA8Unorm, size, 1, true)?,
                depth: render_texture(device, MTLPixelFormat::Depth32Float, size, 1, false)?,
                msaa,
            });
        }
        slot.as_ref()
    }
}

fn set_bytes<T>(encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>, value: &T, index: usize, fragment: bool) {
    let ptr = NonNull::from(value).cast::<c_void>();
    let len = std::mem::size_of::<T>();
    // SAFETY: `value` lives through the call, which copies it.
    unsafe {
        if fragment {
            encoder.setFragmentBytes_length_atIndex(ptr, len, index);
        } else {
            encoder.setVertexBytes_length_atIndex(ptr, len, index);
        }
    }
}

/// Draws the objects into the control's texture in one pass and hands it to Skia as an image.
pub(in super::super) fn draw(target: &mut Option<Target>, gpu: &mut Gpu, items: &[Item], style: &Style, view_proj: Mat4, size: ISize) -> Option<Image> {
    if size.width <= 0 || size.height <= 0 {
        return None;
    }
    let epoch = gpu.epoch();
    CONTEXT.with_borrow_mut(|slot| {
        if slot.as_ref().is_some_and(|c| c.epoch != epoch) {
            *slot = None;
        }
        if slot.is_none() {
            *slot = Context::new(gpu, epoch);
        }
        let context = slot.as_mut()?;
        for image in items.iter().flat_map(|item| item.materials.iter().filter_map(|m| m.image.as_ref())) {
            context.image(image);
        }
        let models: Vec<Rc<Model>> = items.iter().map(|item| context.model(item.scene)).collect();
        let context = &*context;
        let frame_target = context.target(target, size, style.antialias)?;
        let msaa = frame_target.msaa.is_some();

        let pass = MTLRenderPassDescriptor::renderPassDescriptor();
        // SAFETY: index 0 always exists.
        let color = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        color.setLoadAction(MTLLoadAction::Clear);
        color.setClearColor(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 0.0 });
        let depth = pass.depthAttachment();
        depth.setLoadAction(MTLLoadAction::Clear);
        depth.setClearDepth(1.0);
        depth.setStoreAction(MTLStoreAction::DontCare);
        match &frame_target.msaa {
            Some((ms_color, ms_depth)) => {
                color.setTexture(Some(ms_color));
                color.setResolveTexture(Some(&frame_target.color));
                color.setStoreAction(MTLStoreAction::MultisampleResolve);
                depth.setTexture(Some(ms_depth));
            }
            None => {
                color.setTexture(Some(&frame_target.color));
                color.setStoreAction(MTLStoreAction::Store);
                depth.setTexture(Some(&frame_target.depth));
            }
        }
        let commands = context.queue.commandBuffer()?;
        let encoder = commands.renderCommandEncoderWithDescriptor(&pass)?;
        encoder.setFrontFacingWinding(MTLWinding::CounterClockwise);

        let light = Vec3::new(0.4, 0.8, 0.6).normalize();
        let mut frame = Frame {
            view_proj: view_proj.to_cols_array(),
            eye: super::eye(view_proj).to_array(),
            light: light.extend(0.0).to_array(),
            outline_color: style.outline_color,
            viewport: [size.width as f32, size.height as f32],
            outline: 0.0,
            look: style.look as i32,
            outline_pass: 0,
            _pad: [0; 3],
        };
        let identity = Mat4::IDENTITY.to_cols_array();
        let mut joints = vec![identity; MAX_JOINTS];
        // SAFETY: the bound objects outlive the encoder; indices match the shaders.
        unsafe {
            encoder.setFragmentSamplerState_atIndex(Some(&context.sampler), 0);
            // Bound for every draw: Metal checks each binding the shaders name.
            encoder.setVertexBytes_length_atIndex(NonNull::from(&identity).cast(), std::mem::size_of_val(&identity), 8);
        }
        // The outline first (back faces pushed out), then opaque, then blended, each in tree order.
        let passes: &[(bool, bool)] = if style.outline > 0.0 { &[(true, false), (false, false), (false, true)] } else { &[(false, false), (false, true)] };
        for &(outline_pass, pass_blend) in passes {
            let Some(pipeline) = context.program.pipelines[pipeline_index(pass_blend, msaa)].as_ref() else { continue };
            encoder.setRenderPipelineState(pipeline);
            encoder.setDepthStencilState(Some(&context.depth[pass_blend as usize]));
            frame.outline = if outline_pass { style.outline } else { 0.0 };
            frame.outline_pass = outline_pass as i32;
            set_bytes(&encoder, &frame, 6, false);
            set_bytes(&encoder, &frame, 0, true);
            for (item, gpu_model) in items.iter().zip(&models) {
                let scene = item.scene;
                for (i, node) in scene.nodes.iter().enumerate() {
                    let (Some(m), Some(primitives)) = (node.mesh, node.mesh.and_then(|m| gpu_model.meshes.get(m))) else { continue };
                    if item.hidden.get(i).copied().unwrap_or(false) {
                        continue;
                    }
                    let skin = node.skin.and_then(|s| scene.skins.get(s));
                    // A skinned mesh is placed by its joints only (glTF ignores its node's
                    // transform); the object's place is in the joints' world matrices.
                    let model = if skin.is_some() { Mat4::IDENTITY } else { item.pose[i] };
                    if let Some(skin) = skin {
                        let used = skin.joints.len().clamp(1, MAX_JOINTS);
                        for (j, (joint, ibm)) in skin.joints.iter().zip(&skin.inverse_bind).take(MAX_JOINTS).enumerate() {
                            joints[j] = (item.pose[*joint] * *ibm).to_cols_array();
                        }
                        // SAFETY: `joints` lives through the call, which copies it.
                        unsafe { encoder.setVertexBytes_length_atIndex(NonNull::from(&joints[0]).cast(), used * 64, 8) };
                    }
                    for (p, gpu_p) in scene.meshes[m].iter().zip(primitives) {
                        let Some(gpu_p) = gpu_p else { continue };
                        let material = p.material.and_then(|i| scene.materials.get(i));
                        let changed = p.material.and_then(|i| item.materials.get(i));
                        if changed.is_some_and(|c| c.hidden) || material.is_some_and(|m| m.blend) != pass_blend {
                            continue;
                        }
                        let texture = match changed.and_then(|c| c.image.as_ref()) {
                            Some(image) => context.images.get(&(Rc::as_ptr(&image.0) as usize)).map(|e| &e.1),
                            None => material.and_then(|m| m.texture).and_then(|t| gpu_model.textures.get(t)),
                        };
                        let object = Object {
                            model: model.to_cols_array(),
                            color: changed.and_then(|c| c.color).unwrap_or(material.map_or([0.8, 0.8, 0.8, 1.0], |m| m.color)),
                            rough: changed.and_then(|c| c.roughness).unwrap_or(material.map_or(1.0, |m| m.roughness)),
                            skinned: (skin.is_some() && !p.joints.is_empty()) as i32,
                            textured: texture.is_some() as i32,
                            _pad: 0,
                        };
                        let cull = match (outline_pass, material.is_some_and(|m| m.double_sided)) {
                            (true, _) => MTLCullMode::Front,
                            (false, true) => MTLCullMode::None,
                            (false, false) => MTLCullMode::Back,
                        };
                        encoder.setCullMode(cull);
                        set_bytes(&encoder, &object, 7, false);
                        set_bytes(&encoder, &object, 1, true);
                        // SAFETY: as above; the index buffer holds `count` u32 indices.
                        unsafe {
                            for (index, b) in gpu_p.vertices.iter().enumerate() {
                                encoder.setVertexBuffer_offset_atIndex(Some(b), 0, index);
                            }
                            encoder.setFragmentTexture_atIndex(Some(texture.unwrap_or(&context.blank)), 0);
                            encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(MTLPrimitiveType::Triangle, gpu_p.count, MTLIndexType::UInt32, &gpu_p.indices, 0);
                        }
                    }
                }
            }
        }
        encoder.endEncoding();
        commands.commit();
        let color = Retained::as_ptr(&frame_target.color) as *const c_void;
        // SAFETY: the texture is an `id<MTLTexture>` of Skia's device, of `size`; the control's
        // target keeps it, and Skia keeps what it draws from until its frame ends.
        unsafe { gpu.import_metal_texture(color, size, SurfaceOrigin::TopLeft, ColorType::RGBA8888) }
    })
}
