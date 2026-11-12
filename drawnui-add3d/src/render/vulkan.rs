//! Vulkan (Android): the same drawing as the WebGL2 and Metal paths, on the device and queue Skia
//! draws with. The shaders are the GLSL of `vulkan/*.vert|frag` compiled to SPIR-V
//! (`dev/add3d-shaders.ps1`). Clip space stays GL's, so the image comes out bottom-up as on GL and
//! GL's counter-clockwise front faces read as clockwise here. Each draw records one command buffer,
//! submitted at once on Skia's queue: Skia's work of the frame comes after it in the queue, and the
//! render pass waits for the previous frame's reads of the image before it draws again.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use ash::vk::{self, Handle};
use drawnui::Gpu;
use drawnui::skia::gpu::vk as skia_vk;
use drawnui::skia::{ColorType, ISize, Image, gpu::SurfaceOrigin};
use glam::{Mat4, Vec3};

use super::super::scene::{Scene, Texture3D};
use super::{Item, Style};

/// Joint matrices a skinned mesh may use, as in the other paths.
const MAX_JOINTS: usize = 64;
const VS: &[u8] = include_bytes!("vulkan/model.vert.spv");
const FS: &[u8] = include_bytes!("vulkan/model.frag.spv");
const COLOR_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
/// What the control's image is used for: drawn into, sampled by Skia, and copied from or into
/// (Skia borrows an image only with the transfer usages: it may copy it).
const COLOR_USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
    vk::ImageUsageFlags::COLOR_ATTACHMENT.as_raw() | vk::ImageUsageFlags::SAMPLED.as_raw() | vk::ImageUsageFlags::TRANSFER_SRC.as_raw() | vk::ImageUsageFlags::TRANSFER_DST.as_raw(),
);
const FRAME_BYTES: u64 = std::mem::size_of::<Frame>() as u64;
const JOINT_BYTES: u64 = (MAX_JOINTS * 64) as u64;

/// `Frame` of the shaders (std140): what every object of a pass shares.
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

/// `Object` of the shaders (push constants): one primitive's place and material.
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

fn bytes<T>(v: &[T]) -> &[u8] {
    // SAFETY: plain f32 / u32 arrays and repr(C) structs of them.
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
}

fn align(value: u64, to: u64) -> u64 {
    value.div_ceil(to.max(1)) * to.max(1)
}

/// A buffer in memory the CPU writes (coherent), mapped for its life.
struct Buffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *mut u8,
    size: u64,
}

/// An image with its memory and view.
struct Img {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
}

/// A sampled image: one of a model's, or one put on a material from code.
struct Texture {
    img: Img,
    set: vk::DescriptorSet,
}

/// One primitive on the GPU: its vertex buffers (positions, normals, texture coordinates, joints,
/// weights, outline normals), its indices and their count.
struct Primitive {
    vertices: [Buffer; 6],
    indices: Buffer,
    count: u32,
}

/// One model on the GPU: per mesh its primitives; its textures (index = the scene's image).
struct Model {
    meshes: Vec<Vec<Option<Primitive>>>,
    textures: Vec<Option<Texture>>,
}

/// One command buffer and what it uses until its fence signals: the uniform buffer for the frames
/// and joints of its draw, and the descriptor set pointing at it.
struct Slot {
    commands: vk::CommandBuffer,
    fence: vk::Fence,
    uniforms: Buffer,
    set: vk::DescriptorSet,
    busy: bool,
}

/// What a control's frame holds on the GPU, freed once the GPU is done with it.
struct Parts {
    color: Img,
    depth: Img,
    msaa: Option<(Img, Img)>,
    framebuffer: vk::Framebuffer,
}

/// Frames that were replaced or whose control went, with the frame serial of their last use.
type Retired = Rc<RefCell<Vec<(u64, Parts)>>>;

/// A control's frame: the image it draws into (handed to Skia), its depth buffer, and with
/// `antialias` the multisampled color and depth drawn into and resolved into `color`.
pub(in super::super) struct Target {
    epoch: u32,
    size: ISize,
    parts: Option<Parts>,
    /// The frame serial of the last draw into it.
    serial: u64,
    retired: Retired,
}

impl Drop for Target {
    fn drop(&mut self) {
        if let Some(parts) = self.parts.take() {
            self.retired.borrow_mut().push((self.serial, parts));
        }
    }
}

/// The Vulkan objects of one context: the device, the queue, the shaders, pipelines and states,
/// the command buffers and the models uploaded to it.
struct Context {
    epoch: u32,
    /// The device, its shaders, layouts, render passes, sampler and pipelines.
    prep: Prepared,
    pool: vk::CommandPool,
    slots: Vec<Slot>,
    next: usize,
    descriptor_pools: Vec<vk::DescriptorPool>,
    blank: Texture,
    // ponytail: a model whose scene is gone keeps its buffers until the context goes, as on the
    // other paths.
    models: HashMap<usize, (Weak<Scene>, Rc<Model>)>,
    images: HashMap<usize, (Weak<(Vec<u8>, u32, u32)>, Texture)>,
    retired: Retired,
}

thread_local! {
    static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
}

impl Context {
    /// SAFETY (whole impl): Vulkan calls on the device Skia draws with, from the thread Skia draws
    /// on; every object made here is destroyed in `Drop` after the device is idle.
    fn new(gpu: &Gpu, epoch: u32) -> Option<Self> {
        let handles = gpu.vulkan_handles()?;
        let prep = prepared(handles)?;
        let pool = unsafe {
            prep.device
                .create_command_pool(&vk::CommandPoolCreateInfo::default().queue_family_index(handles.queue_family).flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER), None)
                .ok()?
        };
        let mut context = Self {
            epoch,
            prep,
            pool,
            slots: Vec::new(),
            next: 0,
            descriptor_pools: Vec::new(),
            blank: Texture { img: Img { image: vk::Image::null(), memory: vk::DeviceMemory::null(), view: vk::ImageView::null() }, set: vk::DescriptorSet::null() },
            models: HashMap::new(),
            images: HashMap::new(),
            retired: Rc::new(RefCell::new(Vec::new())),
        };
        context.blank = context.texture(&(vec![255; 4], 1, 1))?;
        Some(context)
    }

    fn memory_type(&self, bits: u32, flags: vk::MemoryPropertyFlags) -> Option<u32> {
        (0..self.prep.memory.memory_type_count).find(|i| bits & (1 << i) != 0 && self.prep.memory.memory_types[*i as usize].property_flags.contains(flags))
    }

    fn buffer(&self, usage: vk::BufferUsageFlags, size: u64) -> Option<Buffer> {
        unsafe {
            let buffer = self.prep.device.create_buffer(&vk::BufferCreateInfo::default().size(size.max(4)).usage(usage), None).ok()?;
            let needs = self.prep.device.get_buffer_memory_requirements(buffer);
            let kind = self.memory_type(needs.memory_type_bits, vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)?;
            let memory = self.prep.device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(needs.size).memory_type_index(kind), None).ok()?;
            self.prep.device.bind_buffer_memory(buffer, memory, 0).ok()?;
            let mapped = self.prep.device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty()).ok()?.cast::<u8>();
            Some(Buffer { buffer, memory, mapped, size: size.max(4) })
        }
    }

    fn buffer_with(&self, usage: vk::BufferUsageFlags, data: &[u8]) -> Option<Buffer> {
        let b = self.buffer(usage, data.len() as u64)?;
        // SAFETY: the mapping holds at least `data.len()` bytes.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), b.mapped, data.len()) };
        Some(b)
    }

    fn free_buffer(&self, b: &Buffer) {
        unsafe {
            self.prep.device.destroy_buffer(b.buffer, None);
            self.prep.device.free_memory(b.memory, None);
        }
    }

    /// An image in GPU memory (lazily allocated where it never leaves the GPU, if the device has it).
    #[allow(clippy::too_many_arguments)]
    fn image(&self, format: vk::Format, size: ISize, levels: u32, samples: vk::SampleCountFlags, usage: vk::ImageUsageFlags, aspect: vk::ImageAspectFlags) -> Option<Img> {
        unsafe {
            let info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D { width: size.width as u32, height: size.height as u32, depth: 1 })
                .mip_levels(levels)
                .array_layers(1)
                .samples(samples)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(usage)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = self.prep.device.create_image(&info, None).ok()?;
            let needs = self.prep.device.get_image_memory_requirements(image);
            let transient = usage.contains(vk::ImageUsageFlags::TRANSIENT_ATTACHMENT);
            let kind = transient
                .then(|| self.memory_type(needs.memory_type_bits, vk::MemoryPropertyFlags::LAZILY_ALLOCATED))
                .flatten()
                .or_else(|| self.memory_type(needs.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL))?;
            let memory = self.prep.device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(needs.size).memory_type_index(kind), None).ok()?;
            self.prep.device.bind_image_memory(image, memory, 0).ok()?;
            let range = vk::ImageSubresourceRange::default().aspect_mask(aspect).level_count(levels).layer_count(1);
            let view = self
                .prep
                .device
                .create_image_view(&vk::ImageViewCreateInfo::default().image(image).view_type(vk::ImageViewType::TYPE_2D).format(format).subresource_range(range), None)
                .ok()?;
            Some(Img { image, memory, view })
        }
    }

    fn free_image(&self, img: &Img) {
        unsafe {
            self.prep.device.destroy_image_view(img.view, None);
            self.prep.device.destroy_image(img.image, None);
            self.prep.device.free_memory(img.memory, None);
        }
    }

    fn free_parts(&self, parts: &Parts) {
        unsafe { self.prep.device.destroy_framebuffer(parts.framebuffer, None) };
        self.free_image(&parts.color);
        self.free_image(&parts.depth);
        if let Some((c, d)) = &parts.msaa {
            self.free_image(c);
            self.free_image(d);
        }
    }

    /// A descriptor set of `layout`, from a pool with room (a new pool when they are full).
    fn descriptor_set(&mut self, layout: vk::DescriptorSetLayout) -> Option<vk::DescriptorSet> {
        let layouts = [layout];
        unsafe {
            if let Some(pool) = self.descriptor_pools.last()
                && let Ok(sets) = self.prep.device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default().descriptor_pool(*pool).set_layouts(&layouts))
            {
                return sets.first().copied();
            }
            let sizes = [
                vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 256 },
                vk::DescriptorPoolSize { ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC, descriptor_count: 128 },
            ];
            let pool = self.prep.device.create_descriptor_pool(&vk::DescriptorPoolCreateInfo::default().max_sets(256).pool_sizes(&sizes), None).ok()?;
            self.descriptor_pools.push(pool);
            self.prep.device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts)).ok()?.first().copied()
        }
    }

    /// Records `record` into a new command buffer, submits it and waits for it (uploads only).
    fn once(&self, record: impl FnOnce(vk::CommandBuffer)) -> Option<()> {
        unsafe {
            let commands = self
                .prep
                .device
                .allocate_command_buffers(&vk::CommandBufferAllocateInfo::default().command_pool(self.pool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(1))
                .ok()?[0];
            self.prep.device.begin_command_buffer(commands, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)).ok()?;
            record(commands);
            self.prep.device.end_command_buffer(commands).ok()?;
            let fence = self.prep.device.create_fence(&vk::FenceCreateInfo::default(), None).ok()?;
            let list = [commands];
            let submit = [vk::SubmitInfo::default().command_buffers(&list)];
            let done = self.prep.device.queue_submit(self.prep.queue, &submit, fence).is_ok() && self.prep.device.wait_for_fences(&[fence], true, u64::MAX).is_ok();
            self.prep.device.destroy_fence(fence, None);
            self.prep.device.free_command_buffers(self.pool, &list);
            done.then_some(())
        }
    }

    /// One RGBA image as a mipmapped texture with its descriptor set.
    fn texture(&mut self, (rgba, w, h): &(Vec<u8>, u32, u32)) -> Option<Texture> {
        let (w, h) = ((*w).max(1), (*h).max(1));
        if rgba.len() < (w * h * 4) as usize {
            return None;
        }
        let levels = 32 - w.max(h).leading_zeros();
        let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC;
        let img = self.image(COLOR_FORMAT, ISize::new(w as i32, h as i32), levels, vk::SampleCountFlags::TYPE_1, usage, vk::ImageAspectFlags::COLOR)?;
        let staging = self.buffer_with(vk::BufferUsageFlags::TRANSFER_SRC, &rgba[..(w * h * 4) as usize])?;
        let image = img.image;
        let device = &self.prep.device;
        let barrier = |commands, level: u32, count: u32, from: vk::ImageLayout, to: vk::ImageLayout, src: vk::AccessFlags, dst: vk::AccessFlags, src_stage, dst_stage| {
            let range = vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).base_mip_level(level).level_count(count).layer_count(1);
            let b = [vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(from)
                .new_layout(to)
                .src_access_mask(src)
                .dst_access_mask(dst)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .subresource_range(range)];
            // SAFETY: recording into `commands`.
            unsafe { device.cmd_pipeline_barrier(commands, src_stage, dst_stage, vk::DependencyFlags::empty(), &[], &[], &b) };
        };
        let uploaded = self.once(|commands| {
            let transfer = vk::PipelineStageFlags::TRANSFER;
            barrier(commands, 0, levels, vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::AccessFlags::empty(), vk::AccessFlags::TRANSFER_WRITE, vk::PipelineStageFlags::TOP_OF_PIPE, transfer);
            let copy = [vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1))
                .image_extent(vk::Extent3D { width: w, height: h, depth: 1 })];
            // SAFETY: recording into `commands`.
            unsafe { device.cmd_copy_buffer_to_image(commands, staging.buffer, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &copy) };
            // Each level from the one above, halved with linear filtering.
            for level in 1..levels {
                barrier(commands, level - 1, 1, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, vk::AccessFlags::TRANSFER_WRITE, vk::AccessFlags::TRANSFER_READ, transfer, transfer);
                let size = |l: u32| [vk::Offset3D::default(), vk::Offset3D { x: (w >> l).max(1) as i32, y: (h >> l).max(1) as i32, z: 1 }];
                let layer = |l| vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).mip_level(l).layer_count(1);
                let blit = [vk::ImageBlit::default().src_subresource(layer(level - 1)).src_offsets(size(level - 1)).dst_subresource(layer(level)).dst_offsets(size(level))];
                // SAFETY: recording into `commands`.
                unsafe { device.cmd_blit_image(commands, image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &blit, vk::Filter::LINEAR) };
                barrier(commands, level - 1, 1, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, vk::AccessFlags::TRANSFER_READ, vk::AccessFlags::SHADER_READ, transfer, vk::PipelineStageFlags::FRAGMENT_SHADER);
            }
            barrier(commands, levels - 1, 1, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, vk::AccessFlags::TRANSFER_WRITE, vk::AccessFlags::SHADER_READ, transfer, vk::PipelineStageFlags::FRAGMENT_SHADER);
        });
        self.free_buffer(&staging);
        if uploaded.is_none() {
            self.free_image(&img);
            return None;
        }
        let set = self.descriptor_set(self.prep.texture_layout)?;
        let info = [vk::DescriptorImageInfo::default().sampler(self.prep.sampler).image_view(img.view).image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write = [vk::WriteDescriptorSet::default().dst_set(set).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&info)];
        unsafe { self.prep.device.update_descriptor_sets(&write, &[]) };
        Some(Texture { img, set })
    }

    /// The scene's model on this context, uploaded the first time any control draws it.
    fn model(&mut self, scene: &Rc<Scene>) -> Option<Rc<Model>> {
        let key = Rc::as_ptr(scene) as usize;
        if let Some((weak, model)) = self.models.get(&key)
            && weak.upgrade().is_some_and(|s| Rc::ptr_eq(&s, scene))
        {
            return Some(model.clone());
        }
        let textures = scene.images.iter().map(|i| self.texture(i)).collect();
        let vertex = vk::BufferUsageFlags::VERTEX_BUFFER;
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
                            self.buffer_with(vertex, bytes(&p.positions))?,
                            self.buffer_with(vertex, &normals)?,
                            self.buffer_with(vertex, &uvs)?,
                            self.buffer_with(vertex, &joints)?,
                            self.buffer_with(vertex, &weights)?,
                            self.buffer_with(vertex, bytes(&outline))?,
                        ];
                        Some(Primitive { vertices, indices: self.buffer_with(vk::BufferUsageFlags::INDEX_BUFFER, bytes(&p.indices))?, count: p.indices.len() as u32 })
                    })
                    .collect()
            })
            .collect();
        let model = Rc::new(Model { meshes, textures });
        self.models.retain(|_, (weak, _)| weak.strong_count() > 0);
        self.models.insert(key, (Rc::downgrade(scene), model.clone()));
        Some(model)
    }

    /// The texture of an image put on a material, uploaded the first time it is drawn.
    fn material_image(&mut self, image: &Texture3D) {
        let key = Rc::as_ptr(&image.0) as usize;
        if let Some((weak, _)) = self.images.get(&key)
            && weak.upgrade().is_some_and(|i| Rc::ptr_eq(&i, &image.0))
        {
            return;
        }
        let Some(t) = self.texture(&image.0) else { return };
        self.images.retain(|_, (weak, _)| weak.strong_count() > 0);
        self.images.insert(key, (Rc::downgrade(&image.0), t));
    }

    /// Frees what the GPU finished with: replaced or dropped frames up to `completed`.
    fn free_retired(&mut self, completed: u64) {
        let done: Vec<Parts> = {
            let mut list = self.retired.borrow_mut();
            let (done, keep): (Vec<_>, Vec<_>) = list.drain(..).partition(|(serial, _)| *serial <= completed);
            *list = keep;
            done.into_iter().map(|(_, parts)| parts).collect()
        };
        for parts in &done {
            self.free_parts(parts);
        }
    }

    /// The control's frame at `size` for this context, made again when either changed; the
    /// multisampled part with `antialias` on a device that has it.
    fn target(&self, slot: &mut Option<Target>, size: ISize, antialias: bool, serial: u64) -> Option<()> {
        let msaa = antialias && self.prep.samples != vk::SampleCountFlags::TYPE_1;
        let fits = |t: &Target| t.epoch == self.epoch && t.size == size && t.parts.as_ref().is_some_and(|p| p.msaa.is_some() == msaa);
        if slot.as_ref().is_some_and(fits) {
            return Some(());
        }
        // The old one is freed once the GPU finished the frames that drew it (its Drop).
        if let Some(old) = slot.as_mut() {
            old.serial = old.serial.max(serial.saturating_sub(1));
        }
        *slot = None;
        let color_usage = COLOR_USAGE;
        let depth_usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT;
        let depth_aspect = vk::ImageAspectFlags::DEPTH;
        let color = self.image(COLOR_FORMAT, size, 1, vk::SampleCountFlags::TYPE_1, color_usage, vk::ImageAspectFlags::COLOR)?;
        let depth = self.image(self.prep.depth_format, size, 1, vk::SampleCountFlags::TYPE_1, depth_usage, depth_aspect)?;
        let ms = match msaa {
            true => Some((
                self.image(COLOR_FORMAT, size, 1, self.prep.samples, vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT, vk::ImageAspectFlags::COLOR)?,
                self.image(self.prep.depth_format, size, 1, self.prep.samples, depth_usage, depth_aspect)?,
            )),
            false => None,
        };
        let views: Vec<vk::ImageView> = match &ms {
            Some((c, d)) => vec![c.view, d.view, color.view],
            None => vec![color.view, depth.view],
        };
        let pass = self.prep.passes[msaa as usize];
        let framebuffer = unsafe {
            self.prep.device
                .create_framebuffer(
                    &vk::FramebufferCreateInfo::default().render_pass(pass).attachments(&views).width(size.width as u32).height(size.height as u32).layers(1),
                    None,
                )
                .ok()?
        };
        *slot = Some(Target { epoch: self.epoch, size, parts: Some(Parts { color, depth, msaa: ms, framebuffer }), serial, retired: self.retired.clone() });
        Some(())
    }

    /// A free command buffer slot whose uniform buffer holds `bytes`: the oldest one when the GPU
    /// finished it, else a new one (no waiting for the GPU).
    fn slot(&mut self, bytes: u64) -> Option<usize> {
        let free = (0..self.slots.len()).map(|i| (self.next + i) % self.slots.len().max(1)).find(|i| {
            let s = &self.slots[*i];
            // SAFETY: the slot's fence.
            !s.busy || unsafe { self.prep.device.get_fence_status(s.fence) }.unwrap_or(false)
        });
        let index = match free {
            Some(i) => i,
            None => {
                let commands = unsafe {
                    self.prep.device
                        .allocate_command_buffers(&vk::CommandBufferAllocateInfo::default().command_pool(self.pool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(1))
                        .ok()?[0]
                };
                let fence = unsafe { self.prep.device.create_fence(&vk::FenceCreateInfo::default(), None).ok()? };
                let uniforms = self.buffer(vk::BufferUsageFlags::UNIFORM_BUFFER, bytes.max(64 * 1024))?;
                let set = self.descriptor_set(self.prep.frame_layout)?;
                self.slots.push(Slot { commands, fence, uniforms, set, busy: false });
                self.write_frame_set(self.slots.len() - 1);
                self.slots.len() - 1
            }
        };
        let slot = &mut self.slots[index];
        if slot.busy {
            // SAFETY: signaled, nothing waits on it.
            unsafe { self.prep.device.reset_fences(&[slot.fence]).ok()? };
            slot.busy = false;
        }
        if slot.uniforms.size < bytes {
            let bigger = self.buffer(vk::BufferUsageFlags::UNIFORM_BUFFER, bytes.next_power_of_two())?;
            let old = std::mem::replace(&mut self.slots[index].uniforms, bigger);
            self.free_buffer(&old);
            self.write_frame_set(index);
        }
        self.next = (index + 1) % self.slots.len();
        Some(index)
    }

    /// Points the slot's descriptor set at its uniform buffer (the frame and the joints).
    fn write_frame_set(&self, index: usize) {
        let s = &self.slots[index];
        let frame = [vk::DescriptorBufferInfo { buffer: s.uniforms.buffer, offset: 0, range: FRAME_BYTES }];
        let joints = [vk::DescriptorBufferInfo { buffer: s.uniforms.buffer, offset: 0, range: JOINT_BYTES }];
        let writes = [
            vk::WriteDescriptorSet::default().dst_set(s.set).dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC).buffer_info(&frame),
            vk::WriteDescriptorSet::default().dst_set(s.set).dst_binding(1).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC).buffer_info(&joints),
        ];
        // SAFETY: the set is not in use (a new or finished slot).
        unsafe { self.prep.device.update_descriptor_sets(&writes, &[]) };
    }
}

/// A render pass that clears color and depth and leaves the color ready to sample; multisampled,
/// it resolves into the single-sampled color.
fn render_pass(device: &ash::Device, samples: vk::SampleCountFlags, depth_format: vk::Format) -> Option<vk::RenderPass> {
    let multi = samples != vk::SampleCountFlags::TYPE_1;
    let color = vk::AttachmentDescription::default()
        .format(COLOR_FORMAT)
        .samples(samples)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(if multi { vk::AttachmentStoreOp::DONT_CARE } else { vk::AttachmentStoreOp::STORE })
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(if multi { vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL } else { vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL });
    let depth = vk::AttachmentDescription::default()
        .format(depth_format)
        .samples(samples)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
    let resolve = vk::AttachmentDescription::default()
        .format(COLOR_FORMAT)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::DONT_CARE)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    let attachments: Vec<_> = if multi { vec![color, depth, resolve] } else { vec![color, depth] };
    let color_ref = [vk::AttachmentReference { attachment: 0, layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL }];
    let depth_ref = vk::AttachmentReference { attachment: 1, layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL };
    let resolve_ref = [vk::AttachmentReference { attachment: 2, layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL }];
    let mut subpass = vk::SubpassDescription::default().pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS).color_attachments(&color_ref).depth_stencil_attachment(&depth_ref);
    if multi {
        subpass = subpass.resolve_attachments(&resolve_ref);
    }
    let draw_stages = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS;
    let dependencies = [
        // The previous frame's reads of the image (Skia sampling it) end before it is drawn again.
        vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER | draw_stages)
            .dst_stage_mask(draw_stages)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE),
        // The drawing ends before Skia samples the image.
        vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ),
    ];
    let subpasses = [subpass];
    let info = vk::RenderPassCreateInfo::default().attachments(&attachments).subpasses(&subpasses).dependencies(&dependencies);
    // SAFETY: a valid description on the live device.
    unsafe { device.create_render_pass(&info, None) }.ok()
}

/// What a context needs that depends only on the device: shaders, layouts, render passes, the
/// sampler and every pipeline. Made on a thread of its own as soon as a 3D control exists
/// (`start_program`), so the first frame finds them; made in the first draw when no Vulkan context
/// existed yet.
struct Prepared {
    device_handle: u64,
    device: ash::Device,
    queue: vk::Queue,
    memory: vk::PhysicalDeviceMemoryProperties,
    uniform_align: u64,
    texture_layout: vk::DescriptorSetLayout,
    frame_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    shaders: [vk::ShaderModule; 2],
    cache: vk::PipelineCache,
    passes: [vk::RenderPass; 2],
    samples: vk::SampleCountFlags,
    depth_format: vk::Format,
    sampler: vk::Sampler,
    pipelines: HashMap<(bool, bool, i32), vk::Pipeline>,
}

/// SAFETY (whole impl): Vulkan calls on the device Skia draws with; the objects made here are
/// destroyed by the context that takes them over (`Drop for Context`).
impl Prepared {
    fn new(handles: drawnui::VulkanHandles) -> Option<Self> {
        unsafe {
            let entry = ash::Entry::load().ok()?;
            let instance = ash::Instance::load(entry.static_fn(), vk::Instance::from_raw(handles.instance));
            let physical = vk::PhysicalDevice::from_raw(handles.physical_device);
            let device = ash::Device::load(instance.fp_v1_0(), vk::Device::from_raw(handles.device));
            let memory = instance.get_physical_device_memory_properties(physical);
            let limits = instance.get_physical_device_properties(physical).limits;
            let samples = [vk::SampleCountFlags::TYPE_4, vk::SampleCountFlags::TYPE_2]
                .into_iter()
                .find(|s| limits.framebuffer_color_sample_counts.contains(*s) && limits.framebuffer_depth_sample_counts.contains(*s))
                .unwrap_or(vk::SampleCountFlags::TYPE_1);
            let depth_format = [vk::Format::D32_SFLOAT, vk::Format::X8_D24_UNORM_PACK32, vk::Format::D24_UNORM_S8_UINT, vk::Format::D16_UNORM]
                .into_iter()
                .find(|f| instance.get_physical_device_format_properties(physical, *f).optimal_tiling_features.contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT))?;
            let texture_binding = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
            let texture_layout = device.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&texture_binding), None).ok()?;
            let frame_bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::VERTEX),
            ];
            let frame_layout = device.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&frame_bindings), None).ok()?;
            let push = [vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                .size(std::mem::size_of::<Object>() as u32)];
            let layouts = [texture_layout, frame_layout];
            let pipeline_layout = device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts).push_constant_ranges(&push), None).ok()?;
            let module = |code: &[u8]| {
                let words = ash::util::read_spv(&mut std::io::Cursor::new(code)).ok()?;
                device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None).ok()
            };
            let shaders = [module(VS)?, module(FS)?];
            let cache = device.create_pipeline_cache(&vk::PipelineCacheCreateInfo::default(), None).ok()?;
            let single = render_pass(&device, vk::SampleCountFlags::TYPE_1, depth_format)?;
            let multi = if samples != vk::SampleCountFlags::TYPE_1 { render_pass(&device, samples, depth_format)? } else { vk::RenderPass::null() };
            let sampler = device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::REPEAT)
                        .address_mode_v(vk::SamplerAddressMode::REPEAT)
                        .address_mode_w(vk::SamplerAddressMode::REPEAT)
                        .max_lod(vk::LOD_CLAMP_NONE)
                        // Text and fine lines on a surface at an angle (a tilted card) stay sharp.
                        .anisotropy_enable(handles.sampler_anisotropy)
                        .max_anisotropy(if handles.sampler_anisotropy { limits.max_sampler_anisotropy.min(8.0) } else { 1.0 }),
                    None,
                )
                .ok()?;
            let mut prepared = Self {
                device_handle: handles.device,
                device,
                queue: vk::Queue::from_raw(handles.queue),
                memory,
                uniform_align: limits.min_uniform_buffer_offset_alignment.max(16),
                texture_layout,
                frame_layout,
                pipeline_layout,
                shaders,
                cache,
                passes: [single, multi],
                samples,
                depth_format,
                sampler,
                pipelines: HashMap::new(),
            };
            // Every pipeline a draw can ask for: single sampled and multisampled, opaque and
            // blended, each cull mode.
            let multisampled: &[bool] = if samples != vk::SampleCountFlags::TYPE_1 { &[false, true] } else { &[false] };
            for &msaa in multisampled {
                for blend in [false, true] {
                    for cull in [vk::CullModeFlags::BACK, vk::CullModeFlags::FRONT, vk::CullModeFlags::NONE] {
                        prepared.pipeline(msaa, blend, cull)?;
                    }
                }
            }
            Some(prepared)
        }
    }

    /// The pipeline for a pass: multisampled or not, blended or not, its cull mode.
    fn pipeline(&mut self, msaa: bool, blend: bool, cull: vk::CullModeFlags) -> Option<vk::Pipeline> {
        let key = (msaa, blend, cull.as_raw() as i32);
        if let Some(p) = self.pipelines.get(&key) {
            return Some(*p);
        }
        let entry = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(self.shaders[0]).name(entry),
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(self.shaders[1]).name(entry),
        ];
        let strides = [12u32, 12, 8, 16, 16, 12];
        let formats = [
            vk::Format::R32G32B32_SFLOAT,
            vk::Format::R32G32B32_SFLOAT,
            vk::Format::R32G32_SFLOAT,
            vk::Format::R32G32B32A32_SFLOAT,
            vk::Format::R32G32B32A32_SFLOAT,
            vk::Format::R32G32B32_SFLOAT,
        ];
        let bindings: Vec<_> = (0..6).map(|i| vk::VertexInputBindingDescription { binding: i, stride: strides[i as usize], input_rate: vk::VertexInputRate::VERTEX }).collect();
        let attributes: Vec<_> = (0..6).map(|i| vk::VertexInputAttributeDescription { location: i, binding: i, format: formats[i as usize], offset: 0 }).collect();
        let input = vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(&bindings).vertex_attribute_descriptions(&attributes);
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(cull)
            .front_face(vk::FrontFace::CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(if msaa { self.samples } else { vk::SampleCountFlags::TYPE_1 });
        let depth = vk::PipelineDepthStencilStateCreateInfo::default().depth_test_enable(true).depth_write_enable(!blend).depth_compare_op(vk::CompareOp::LESS);
        let attachment = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(blend)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blending = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachment);
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let info = [vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&input)
            .input_assembly_state(&assembly)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth)
            .color_blend_state(&blending)
            .dynamic_state(&dynamic)
            .layout(self.pipeline_layout)
            .render_pass(self.passes[msaa as usize])];
        // SAFETY: valid descriptions on the live device; the cache is used by this owner only.
        let pipeline = unsafe { self.device.create_graphics_pipelines(self.cache, &info, None) }
            .map_err(|(_, e)| eprintln!("SkiaModel3D pipeline: {e}"))
            .ok()?[0];
        self.pipelines.insert(key, pipeline);
        Some(pipeline)
    }
}

static START: std::sync::Once = std::sync::Once::new();
static STARTED: std::sync::Mutex<Option<std::thread::JoinHandle<Option<Prepared>>>> = std::sync::Mutex::new(None);

// SAFETY: plain Vulkan handles and the device's function table, used by one thread at a time.
unsafe impl Send for Prepared {}

/// Builds the shaders, render passes and every pipeline on a thread of its own as soon as a 3D
/// control exists and the host made its Vulkan context, before anything is drawn (as games compile
/// their shaders while loading). Once per process.
pub(in super::super) fn start_program() {
    let Some(handles) = Gpu::last_vulkan_handles() else { return };
    START.call_once(|| {
        let job = std::thread::spawn(move || Prepared::new(handles));
        if let Ok(mut slot) = STARTED.lock() {
            *slot = Some(job);
        }
    });
}

/// The device-level objects for `handles`: the ones started early when they were made on that
/// device, else made now.
fn prepared(handles: drawnui::VulkanHandles) -> Option<Prepared> {
    let started = STARTED.lock().ok().and_then(|mut slot| slot.take()).and_then(|job| job.join().ok()).flatten();
    match started {
        Some(p) if p.device_handle == handles.device => Some(p),
        _ => Prepared::new(handles),
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: nothing of this context is used after it goes; the device finishes first.
        unsafe {
            let _ = self.prep.device.device_wait_idle();
            let retired: Vec<_> = self.retired.borrow_mut().drain(..).collect();
            for (_, parts) in &retired {
                self.free_parts(parts);
            }
            let models: Vec<_> = self.models.drain().map(|(_, (_, model))| model).collect();
            for model in models {
                for p in model.meshes.iter().flatten().flatten() {
                    for b in &p.vertices {
                        self.free_buffer(b);
                    }
                    self.free_buffer(&p.indices);
                }
                for t in model.textures.iter().flatten() {
                    self.free_image(&t.img);
                }
            }
            let images: Vec<_> = self.images.drain().map(|(_, (_, t))| t).collect();
            for t in images {
                self.free_image(&t.img);
            }
            self.free_image(&self.blank.img);
            for s in &self.slots {
                self.prep.device.destroy_fence(s.fence, None);
                self.free_buffer(&s.uniforms);
            }
            self.prep.device.destroy_command_pool(self.pool, None);
            for p in self.prep.pipelines.values() {
                self.prep.device.destroy_pipeline(*p, None);
            }
            for pass in self.prep.passes {
                if pass != vk::RenderPass::null() {
                    self.prep.device.destroy_render_pass(pass, None);
                }
            }
            for pool in &self.descriptor_pools {
                self.prep.device.destroy_descriptor_pool(*pool, None);
            }
            self.prep.device.destroy_sampler(self.prep.sampler, None);
            self.prep.device.destroy_pipeline_cache(self.prep.cache, None);
            for m in self.prep.shaders {
                self.prep.device.destroy_shader_module(m, None);
            }
            self.prep.device.destroy_pipeline_layout(self.prep.pipeline_layout, None);
            self.prep.device.destroy_descriptor_set_layout(self.prep.texture_layout, None);
            self.prep.device.destroy_descriptor_set_layout(self.prep.frame_layout, None);
        }
    }
}

/// Draws the objects into the control's image in one pass and hands it to Skia.
pub(in super::super) fn draw(target: &mut Option<Target>, gpu: &mut Gpu, items: &[Item], style: &Style, view_proj: Mat4, size: ISize) -> Option<Image> {
    if size.width <= 0 || size.height <= 0 {
        return None;
    }
    let epoch = gpu.epoch();
    let serial = gpu.frame_serial();
    let completed = gpu.completed_serial();
    let family = gpu.vulkan_handles()?.queue_family;
    let color_image = CONTEXT.with_borrow_mut(|slot| -> Option<vk::Image> {
        if slot.as_ref().is_some_and(|c| c.epoch != epoch) {
            *slot = None;
        }
        if slot.is_none() {
            *slot = Context::new(gpu, epoch);
        }
        let context = slot.as_mut()?;
        context.free_retired(completed);
        for image in items.iter().flat_map(|item| item.materials.iter().filter_map(|m| m.image.as_ref())) {
            context.material_image(image);
        }
        let models: Vec<Rc<Model>> = items.iter().map(|item| context.model(item.scene)).collect::<Option<_>>()?;
        context.target(target, size, style.antialias, serial)?;
        let frame_target = target.as_mut()?;
        frame_target.serial = serial;
        let parts = frame_target.parts.as_ref()?;
        let msaa = parts.msaa.is_some();

        // The uniforms of this draw: the frame of each pass, then the joints of each skinned node.
        let passes: &[(bool, bool)] = if style.outline > 0.0 { &[(true, false), (false, false), (false, true)] } else { &[(false, false), (false, true)] };
        let frame_step = align(FRAME_BYTES, context.prep.uniform_align);
        let joint_step = align(JOINT_BYTES, context.prep.uniform_align);
        let skinned: Vec<(usize, usize)> = items
            .iter()
            .enumerate()
            .flat_map(|(o, item)| item.scene.nodes.iter().enumerate().filter(|(_, n)| n.mesh.is_some() && n.skin.is_some()).map(move |(i, _)| (o, i)))
            .collect();
        let bytes_needed = frame_step * passes.len() as u64 + joint_step * (skinned.len() as u64 + 1);
        let index = context.slot(bytes_needed)?;
        let (commands, uniforms, set, fence) = {
            let s = &context.slots[index];
            (s.commands, s.uniforms.mapped, s.set, s.fence)
        };
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
        // SAFETY (writes below): inside the slot's mapping, which holds `bytes_needed`.
        let write = |offset: u64, data: &[u8]| unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), uniforms.add(offset as usize), data.len()) };
        for (p, &(outline_pass, _)) in passes.iter().enumerate() {
            frame.outline = if outline_pass { style.outline } else { 0.0 };
            frame.outline_pass = outline_pass as i32;
            write(frame_step * p as u64, bytes(std::slice::from_ref(&frame)));
        }
        let joints_base = frame_step * passes.len() as u64;
        // The joints of the first offset: identity, for the draws that are not skinned.
        let identity = vec![Mat4::IDENTITY.to_cols_array(); MAX_JOINTS];
        write(joints_base, bytes(&identity));
        let mut joint_offsets: HashMap<(usize, usize), u64> = HashMap::new();
        for (k, &(o, i)) in skinned.iter().enumerate() {
            let item = &items[o];
            let Some(skin) = item.scene.nodes[i].skin.and_then(|s| item.scene.skins.get(s)) else { continue };
            let mut matrices = identity.clone();
            for (j, (joint, ibm)) in skin.joints.iter().zip(&skin.inverse_bind).take(MAX_JOINTS).enumerate() {
                matrices[j] = (item.pose[*joint] * *ibm).to_cols_array();
            }
            let offset = joints_base + joint_step * (k as u64 + 1);
            write(offset, bytes(&matrices));
            joint_offsets.insert((o, i), offset);
        }

        let device = context.prep.device.clone();
        // SAFETY: recording into a finished, reset slot; every object used lives in the context or
        // the target until the slot's fence signals.
        unsafe {
            device.reset_command_buffer(commands, vk::CommandBufferResetFlags::empty()).ok()?;
            device.begin_command_buffer(commands, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)).ok()?;
            let clear = [
                vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } },
                vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } },
                vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } },
            ];
            let area = vk::Rect2D { offset: vk::Offset2D::default(), extent: vk::Extent2D { width: size.width as u32, height: size.height as u32 } };
            let begin = vk::RenderPassBeginInfo::default()
                .render_pass(context.prep.passes[msaa as usize])
                .framebuffer(parts.framebuffer)
                .render_area(area)
                .clear_values(&clear[..if msaa { 3 } else { 2 }]);
            device.cmd_begin_render_pass(commands, &begin, vk::SubpassContents::INLINE);
            device.cmd_set_viewport(commands, 0, &[vk::Viewport { x: 0.0, y: 0.0, width: size.width as f32, height: size.height as f32, min_depth: 0.0, max_depth: 1.0 }]);
            device.cmd_set_scissor(commands, 0, &[area]);
            let layout = context.prep.pipeline_layout;
            for (p, &(outline_pass, pass_blend)) in passes.iter().enumerate() {
                let mut bound: Option<vk::Pipeline> = None;
                for (o, (item, gpu_model)) in items.iter().zip(&models).enumerate() {
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
                        let joints = joint_offsets.get(&(o, i)).copied().unwrap_or(joints_base);
                        let offsets = [(frame_step * p as u64) as u32, joints as u32];
                        device.cmd_bind_descriptor_sets(commands, vk::PipelineBindPoint::GRAPHICS, layout, 1, &[set], &offsets);
                        for (prim, gpu_p) in scene.meshes[m].iter().zip(primitives) {
                            let Some(gpu_p) = gpu_p else { continue };
                            let material = prim.material.and_then(|i| scene.materials.get(i));
                            let changed = prim.material.and_then(|i| item.materials.get(i));
                            if changed.is_some_and(|c| c.hidden) || material.is_some_and(|m| m.blend) != pass_blend {
                                continue;
                            }
                            let texture: Option<vk::DescriptorSet> = match changed.and_then(|c| c.image.as_ref()) {
                                Some(image) => context.images.get(&(Rc::as_ptr(&image.0) as usize)).map(|e| e.1.set),
                                None => material.and_then(|m| m.texture).and_then(|t| gpu_model.textures.get(t)).and_then(Option::as_ref).map(|t| t.set),
                            };
                            let cull = match (outline_pass, material.is_some_and(|m| m.double_sided)) {
                                (true, _) => vk::CullModeFlags::FRONT,
                                (false, true) => vk::CullModeFlags::NONE,
                                (false, false) => vk::CullModeFlags::BACK,
                            };
                            let Some(pipeline) = context.prep.pipeline(msaa, pass_blend, cull) else { continue };
                            if bound != Some(pipeline) {
                                device.cmd_bind_pipeline(commands, vk::PipelineBindPoint::GRAPHICS, pipeline);
                                bound = Some(pipeline);
                            }
                            let object = Object {
                                model: model.to_cols_array(),
                                color: changed.and_then(|c| c.color).unwrap_or(material.map_or([0.8, 0.8, 0.8, 1.0], |m| m.color)),
                                rough: changed.and_then(|c| c.roughness).unwrap_or(material.map_or(1.0, |m| m.roughness)),
                                skinned: (skin.is_some() && !prim.joints.is_empty()) as i32,
                                textured: texture.is_some() as i32,
                                _pad: 0,
                            };
                            device.cmd_push_constants(commands, layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, bytes(std::slice::from_ref(&object)));
                            let texture_set = texture.unwrap_or(context.blank.set);
                            device.cmd_bind_descriptor_sets(commands, vk::PipelineBindPoint::GRAPHICS, layout, 0, &[texture_set], &[]);
                            let buffers: Vec<vk::Buffer> = gpu_p.vertices.iter().map(|b| b.buffer).collect();
                            device.cmd_bind_vertex_buffers(commands, 0, &buffers, &[0; 6]);
                            device.cmd_bind_index_buffer(commands, gpu_p.indices.buffer, 0, vk::IndexType::UINT32);
                            device.cmd_draw_indexed(commands, gpu_p.count, 1, 0, 0, 0);
                        }
                    }
                }
            }
            device.cmd_end_render_pass(commands);
            device.end_command_buffer(commands).ok()?;
            let list = [commands];
            let submit = [vk::SubmitInfo::default().command_buffers(&list)];
            device.queue_submit(context.prep.queue, &submit, fence).ok()?;
        }
        context.slots[index].busy = true;
        Some(parts.color.image)
    })?;
    // SAFETY: the image is the control's, of Skia's device, in SHADER_READ_ONLY_OPTIMAL after the
    // render pass submitted above on Skia's queue; it lives until the GPU finished this frame.
    let info = unsafe {
        let mut info = skia_vk::ImageInfo::new(
            color_image.as_raw() as skia_vk::Image,
            skia_vk::Alloc::default(),
            skia_vk::ImageTiling::OPTIMAL,
            skia_vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            skia_vk::Format::R8G8B8A8_UNORM,
            1,
            Some(family),
            None,
            None,
            None,
        );
        info.image_usage_flags = COLOR_USAGE.as_raw();
        info.sample_count = 1;
        info
    };
    // SAFETY: as above.
    unsafe { gpu.import_vulkan_image(&info, size, SurfaceOrigin::BottomLeft, ColorType::RGBA8888) }
}
