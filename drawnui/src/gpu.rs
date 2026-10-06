//! The only module that names the Skia GPU engine. Controls see `Canvas`, `Image`, `Picture`;
//! another engine (Graphite) changes this file and a host, nothing else.

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
use skia_safe::gpu::gl::FramebufferInfo;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use skia_safe::gpu::mtl;
use skia_safe::{
    Canvas, ColorType, IRect, Image, ImageInfo, Surface,
    gpu::{self, Budgeted, DirectContext, SurfaceOrigin},
    surfaces,
};

pub struct Gpu {
    /// `None` = CPU raster (headless tests).
    ctx: Option<DirectContext>,
    epoch: u32,
}

impl Gpu {
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub(crate) fn new_gl(interface: gpu::gl::Interface) -> Option<Self> {
        let ctx = gpu::direct_contexts::make_gl(interface, None)?;
        Some(Self { ctx: Some(ctx), epoch: 0 })
    }

    /// No GPU: every surface is CPU memory.
    pub fn raster() -> Self {
        Self { ctx: None, epoch: 0 }
    }

    /// Wraps the default framebuffer of the current GL context.
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub(crate) fn window_surface(&mut self, width: i32, height: i32, samples: usize, stencil: usize) -> Option<Surface> {
        let fb = FramebufferInfo {
            fboid: 0,
            format: gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        };
        let target = gpu::backend_render_targets::make_gl((width, height), samples, stencil, fb);
        gpu::surfaces::wrap_backend_render_target(
            self.ctx.as_mut()?,
            &target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        )
    }

    /// Offscreen surface for image caches, on the same engine as the window.
    pub fn offscreen(&mut self, width: i32, height: i32) -> Option<Surface> {
        let info = ImageInfo::new_n32_premul((width, height), None);
        match &mut self.ctx {
            Some(ctx) => gpu::surfaces::render_target(ctx, Budgeted::Yes, &info, None, SurfaceOrigin::TopLeft, None, false, None),
            None => surfaces::raster(&info, None, None),
        }
    }

    pub fn snapshot(&mut self, surface: &mut Surface, subset: Option<IRect>) -> Option<Image> {
        match subset {
            Some(rect) => surface.image_snapshot_with_bounds(rect),
            None => Some(surface.image_snapshot()),
        }
    }

    /// A copy of what `canvas` shows inside `bounds` (device pixels), cut to its surface: the
    /// window, or the offscreen surface of an Image cache being recorded. Returns the copy and
    /// the part of `bounds` it covers. `None` while a picture is recorded (no surface) or when
    /// `bounds` is off the surface. On the GPU it is a texture copy inside the frame's commands.
    pub(crate) fn snapshot_canvas(&mut self, canvas: &Canvas, bounds: IRect) -> Option<(Image, IRect)> {
        // SAFETY: the surface is only read, and dropped before the canvas draws again.
        let mut surface = unsafe { canvas.surface() }?;
        let bounds = IRect::intersect(&bounds, &IRect::from_wh(surface.width(), surface.height()))?;
        Some((surface.image_snapshot_with_bounds(bounds)?, bounds))
    }

    /// Waits until the GPU ran what was submitted.
    #[cfg(not(target_os = "emscripten"))]
    pub(crate) fn finish(&mut self) {
        if let Some(ctx) = &mut self.ctx {
            ctx.submit(Some(gpu::SyncCpu::Yes));
        }
    }

    pub(crate) fn end_frame(&mut self, surface: &mut Surface) {
        if let Some(ctx) = &mut self.ctx {
            ctx.flush_and_submit_surface(surface, None);
        }
    }

    /// Changes when the context is recreated; caches made under an older epoch are dead.
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// The context for the window after `lost`'s was lost: the next epoch, so what was made on
    /// the old one is known dead (`Ui` drops it on the next frame). The old one is abandoned.
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub(crate) fn new_gl_after(interface: gpu::gl::Interface, lost: &mut Gpu) -> Option<Self> {
        // No GL call on the lost context: its objects are gone with it.
        if let Some(ctx) = &mut lost.ctx {
            ctx.abandon();
        }
        let mut gpu = Self::new_gl(interface)?;
        gpu.epoch = lost.epoch.wrapping_add(1);
        Some(gpu)
    }

    /// A Metal context on the window's device and command queue.
    ///
    /// SAFETY: `device` is an `MTLDevice` and `queue` an `MTLCommandQueue` of it; both outlive
    /// the context.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub(crate) unsafe fn new_metal(device: mtl::Handle, queue: mtl::Handle) -> Option<Self> {
        let backend = unsafe { mtl::BackendContext::new(device, queue) };
        let ctx = gpu::direct_contexts::make_metal(&backend, None)?;
        Some(Self { ctx: Some(ctx), epoch: 0 })
    }

    /// The Metal context after `lost`'s was lost: the next epoch, as `new_gl_after`.
    ///
    /// SAFETY: as `new_metal`.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub(crate) unsafe fn new_metal_after(device: mtl::Handle, queue: mtl::Handle, lost: &mut Gpu) -> Option<Self> {
        if let Some(ctx) = &mut lost.ctx {
            ctx.abandon();
        }
        let mut gpu = unsafe { Self::new_metal(device, queue) }?;
        gpu.epoch = lost.epoch.wrapping_add(1);
        Some(gpu)
    }

    /// Wraps the texture of a layer's drawable (BGRA8Unorm) for one frame.
    ///
    /// SAFETY: `texture` is an `MTLTexture` that outlives the surface.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub(crate) unsafe fn drawable_surface(&mut self, texture: mtl::Handle, width: i32, height: i32) -> Option<Surface> {
        let info = unsafe { mtl::TextureInfo::new(texture) };
        let target = gpu::backend_render_targets::make_mtl((width, height), &info);
        gpu::surfaces::wrap_backend_render_target(self.ctx.as_mut()?, &target, SurfaceOrigin::TopLeft, ColorType::BGRA8888, None, None)
    }

    /// A Vulkan context on a device the host made (Android). `instance_proc` and `device_proc`
    /// find the instance's and the device's functions (vkGetInstanceProcAddr,
    /// vkGetDeviceProcAddr); `version` is the API version the device is used at.
    ///
    /// SAFETY: the handles are a live instance, one of its physical devices, a device made on it
    /// and a queue of that device's `family`; all outlive the context.
    #[cfg(target_os = "android")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn new_vulkan(
        instance: u64,
        physical: u64,
        device: u64,
        queue: u64,
        family: u32,
        version: u32,
        instance_proc: &dyn Fn(u64, *const std::ffi::c_char) -> *const std::ffi::c_void,
        device_proc: &dyn Fn(u64, *const std::ffi::c_char) -> *const std::ffi::c_void,
        lost: Option<&mut Gpu>,
    ) -> Option<Self> {
        use gpu::vk;
        let get_proc = |of: vk::GetProcOf| match of {
            vk::GetProcOf::Instance(instance, name) => instance_proc(instance as u64, name),
            vk::GetProcOf::Device(device, name) => device_proc(device as u64, name),
        };
        let backend = unsafe {
            vk::BackendContext::new_builder(
                instance as vk::Instance,
                physical as vk::PhysicalDevice,
                device as vk::Device,
                (queue as vk::Queue, family as usize),
                &get_proc,
                Some(vk::Version::from(version)),
            )
            .build()
        };
        let ctx = gpu::direct_contexts::make_vulkan(&backend, None)?;
        let epoch = match lost {
            Some(lost) => {
                if let Some(ctx) = &mut lost.ctx {
                    ctx.abandon();
                }
                lost.epoch.wrapping_add(1)
            }
            None => 0,
        };
        Some(Self { ctx: Some(ctx), epoch })
    }

    /// Wraps a swapchain image (Vulkan, Android) as the window's surface; kept for the image's
    /// life, Skia follows its layout.
    ///
    /// SAFETY: `image` is a `VkImage` of the context's device, of `usage`, that outlives the surface.
    #[cfg(target_os = "android")]
    pub(crate) unsafe fn vulkan_surface(&mut self, image: u64, rgba: bool, usage: u32, width: i32, height: i32) -> Option<Surface> {
        use gpu::vk;
        let (format, color) = if rgba { (vk::Format::R8G8B8A8_UNORM, ColorType::RGBA8888) } else { (vk::Format::B8G8R8A8_UNORM, ColorType::BGRA8888) };
        let mut info = unsafe {
            vk::ImageInfo::new(image as vk::Image, vk::Alloc::default(), vk::ImageTiling::OPTIMAL, vk::ImageLayout::UNDEFINED, format, 1, None, None, None, None)
        };
        info.image_usage_flags = usage;
        info.sample_count = 1;
        let target = gpu::backend_render_targets::make_vk((width, height), &info);
        gpu::surfaces::wrap_backend_render_target(self.ctx.as_mut()?, &target, SurfaceOrigin::TopLeft, color, None, None)
    }

    /// The GPU waits for `semaphore` (the swapchain image is free) before it draws more into
    /// `surface`.
    ///
    /// SAFETY: `semaphore` is a `VkSemaphore` of the device, signaled by an acquire, alive until
    /// the GPU waited on it.
    #[cfg(target_os = "android")]
    pub(crate) unsafe fn vulkan_wait(&mut self, surface: &mut Surface, semaphore: u64) -> bool {
        let wait = unsafe { gpu::backend_semaphores::make_vk(semaphore as gpu::vk::Semaphore) };
        surface.wait(&[wait], false)
    }

    /// Ends a Vulkan frame: the surface goes to the presentable layout, the commands are
    /// submitted and `semaphore` is signaled once they ran. False when nothing will signal it
    /// (presenting must then not wait on it).
    ///
    /// SAFETY: `semaphore` is an unsignaled `VkSemaphore` of the device.
    #[cfg(target_os = "android")]
    pub(crate) unsafe fn vulkan_flush(&mut self, surface: &mut Surface, semaphore: u64) -> bool {
        let Some(ctx) = &mut self.ctx else { return false };
        let mut signal = [unsafe { gpu::backend_semaphores::make_vk(semaphore as gpu::vk::Semaphore) }];
        let mut info = gpu::FlushInfo::default();
        unsafe { info.set_signal_semaphores(&mut signal) };
        let submitted = ctx.flush_surface_with_access(surface, surfaces::BackendSurfaceAccess::Present, &info);
        ctx.submit(gpu::SubmitInfo::default());
        submitted == gpu::SemaphoresSubmitted::Yes
    }

    /// No GPU, as `raster`, standing for a context made after `lost`'s (tests).
    pub fn raster_after(lost: &Gpu) -> Self {
        Self { ctx: None, epoch: lost.epoch.wrapping_add(1) }
    }

    /// The context is lost (a GPU reset, a lost WebGL context): nothing drawn on it shows, it
    /// has to be made again.
    pub(crate) fn lost(&mut self) -> bool {
        self.ctx.as_mut().is_some_and(|ctx| ctx.abandoned())
    }

    /// Frees the context's GPU objects and stops using it (the window closes).
    #[cfg(not(target_os = "emscripten"))]
    pub(crate) fn abandon(&mut self) {
        if let Some(ctx) = &mut self.ctx {
            ctx.release_resources_and_abandon();
        }
    }
}
