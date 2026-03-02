pub mod buffer;
pub mod texture;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator,
                             AllocatorCreateDesc};
use gpu_allocator::MemoryLocation;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::ffi::{CStr, CString};
use std::sync::Arc;
use winit::window::Window;

pub const FRAMES_IN_FLIGHT: usize = 2;

// ── UBO / Push-constant types ─────────────────────────────────────────────

#[repr(C)]
pub struct UBO3D {
    pub view:      [[f32; 4]; 4],
    pub proj:      [[f32; 4]; 4],
    pub light_dir: [f32; 4],
}

#[repr(C)]
pub struct UBOHUD {
    pub screen_size: [f32; 2],
    pub _pad:        [f32; 2],
}

#[repr(C)]
pub struct Push3D {
    pub model:    [[f32; 4]; 4],
    pub color:    [f32; 4],
    pub emission: [f32; 4],
}

#[repr(C)]
pub struct PushTrail {
    pub model: [[f32; 4]; 4],
}

// ── Compiled SPIR-V ───────────────────────────────────────────────────────

static SCENE_VERT: &[u8] = include_bytes!("../../shaders/spirv/scene.vert.spv");
static SCENE_FRAG: &[u8] = include_bytes!("../../shaders/spirv/scene.frag.spv");
static TRAIL_VERT: &[u8] = include_bytes!("../../shaders/spirv/trail.vert.spv");
static TRAIL_FRAG: &[u8] = include_bytes!("../../shaders/spirv/trail.frag.spv");
static HUD_VERT:   &[u8] = include_bytes!("../../shaders/spirv/hud.vert.spv");
static HUD_FRAG:   &[u8] = include_bytes!("../../shaders/spirv/hud.frag.spv");

// ── Main context ──────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct VulkanContext {
    pub entry:           ash::Entry,
    pub instance:        ash::Instance,
    pub surface_loader:  ash::khr::surface::Instance,
    pub surface:         vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub device:          ash::Device,
    pub graphics_queue:  vk::Queue,
    pub present_queue:   vk::Queue,
    pub graphics_family: u32,
    pub present_family:  u32,

    pub swapchain_loader: ash::khr::swapchain::Device,
    pub swapchain:        vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_views:  Vec<vk::ImageView>,
    pub swapchain_format: vk::Format,
    pub extent:           vk::Extent2D,

    pub depth_image: vk::Image,
    pub depth_view:  vk::ImageView,
    pub depth_alloc: Option<Allocation>,

    pub render_pass:  vk::RenderPass,
    pub framebuffers: Vec<vk::Framebuffer>,

    pub cmd_pool:    vk::CommandPool,
    pub cmd_buffers: Vec<vk::CommandBuffer>,

    pub allocator: Allocator,

    pub desc_pool:       vk::DescriptorPool,
    pub desc_layout_3d:  vk::DescriptorSetLayout,
    pub desc_layout_hud: vk::DescriptorSetLayout,
    pub desc_sets_3d:    Vec<vk::DescriptorSet>,
    pub desc_sets_hud:   Vec<vk::DescriptorSet>,

    pub ubo3d_bufs:    Vec<vk::Buffer>,
    pub ubo3d_allocs:  Vec<Option<Allocation>>,
    pub ubohud_bufs:   Vec<vk::Buffer>,
    pub ubohud_allocs: Vec<Option<Allocation>>,

    pub scene_pipeline: vk::Pipeline,
    pub scene_layout:   vk::PipelineLayout,
    pub trail_pipeline: vk::Pipeline,
    pub trail_layout:   vk::PipelineLayout,
    pub hud_pipeline:   vk::Pipeline,
    pub hud_layout:     vk::PipelineLayout,

    pub img_available: Vec<vk::Semaphore>,
    pub render_done:   Vec<vk::Semaphore>,
    pub in_flight:     Vec<vk::Fence>,

    pub current_frame:    usize,
    pub last_image_idx:   u32,
}

impl VulkanContext {
    pub fn new(window: &Arc<Window>) -> Self {
        unsafe { Self::init(window) }
    }

    unsafe fn init(window: &Arc<Window>) -> Self {
        // ── Entry ─────────────────────────────────────────────────────────
        let entry = ash::Entry::load().expect("Vulkan loader not found");

        // ── Instance ──────────────────────────────────────────────────────
        let app_name    = CString::new("FlightScope").unwrap();
        let engine_name = CString::new("NoEngine").unwrap();
        let app_info = vk::ApplicationInfo {
            p_application_name: app_name.as_ptr(),
            application_version: vk::make_api_version(0, 1, 0, 0),
            p_engine_name: engine_name.as_ptr(),
            engine_version: vk::make_api_version(0, 1, 0, 0),
            api_version: vk::API_VERSION_1_2,
            ..Default::default()
        };

        let display_handle = window.display_handle().unwrap().as_raw();
        let mut ext_ptrs = ash_window::enumerate_required_extensions(display_handle)
            .unwrap()
            .to_vec();
        #[cfg(debug_assertions)]
        ext_ptrs.push(ash::ext::debug_utils::NAME.as_ptr());

        #[cfg(debug_assertions)]
        let layer_c = vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()];
        #[cfg(not(debug_assertions))]
        let layer_c: Vec<CString> = vec![];
        let layer_ptrs: Vec<*const i8> = layer_c.iter().map(|s| s.as_ptr()).collect();

        let inst_info = vk::InstanceCreateInfo {
            p_application_info: &app_info,
            enabled_extension_count: ext_ptrs.len() as u32,
            pp_enabled_extension_names: ext_ptrs.as_ptr(),
            enabled_layer_count: layer_ptrs.len() as u32,
            pp_enabled_layer_names: layer_ptrs.as_ptr(),
            ..Default::default()
        };
        let instance = entry.create_instance(&inst_info, None).unwrap();

        // ── Surface ───────────────────────────────────────────────────────
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let surface = ash_window::create_surface(
            &entry, &instance,
            window.display_handle().unwrap().as_raw(),
            window.window_handle().unwrap().as_raw(),
            None,
        ).unwrap();

        // ── Physical device ───────────────────────────────────────────────
        let pds = instance.enumerate_physical_devices().unwrap();
        let (physical_device, graphics_family, present_family) = pds
            .iter()
            .find_map(|&pd| pick_device(&instance, &surface_loader, surface, pd))
            .expect("No Vulkan-capable GPU with swapchain support");

        let props = instance.get_physical_device_properties(physical_device);
        println!("GPU: {}", CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy());

        // ── Logical device ────────────────────────────────────────────────
        let q_pri = [1.0_f32];
        let mut q_infos = vec![vk::DeviceQueueCreateInfo {
            queue_family_index: graphics_family,
            queue_count: 1,
            p_queue_priorities: q_pri.as_ptr(),
            ..Default::default()
        }];
        if present_family != graphics_family {
            q_infos.push(vk::DeviceQueueCreateInfo {
                queue_family_index: present_family,
                queue_count: 1,
                p_queue_priorities: q_pri.as_ptr(),
                ..Default::default()
            });
        }
        let dev_exts = [ash::khr::swapchain::NAME.as_ptr()];
        let dev_features = vk::PhysicalDeviceFeatures::default();
        let dev_info = vk::DeviceCreateInfo {
            queue_create_info_count: q_infos.len() as u32,
            p_queue_create_infos: q_infos.as_ptr(),
            enabled_extension_count: dev_exts.len() as u32,
            pp_enabled_extension_names: dev_exts.as_ptr(),
            p_enabled_features: &dev_features,
            ..Default::default()
        };
        let device = instance.create_device(physical_device, &dev_info, None).unwrap();
        let graphics_queue = device.get_device_queue(graphics_family, 0);
        let present_queue  = device.get_device_queue(present_family, 0);

        // ── Swapchain ─────────────────────────────────────────────────────
        let (sc_loader, swapchain, sc_images, sc_format, extent) = create_swapchain(
            &instance, &device, &surface_loader, surface,
            physical_device, graphics_family, present_family, window,
        );
        let sc_views: Vec<_> = sc_images
            .iter()
            .map(|&img| make_image_view(&device, img, sc_format, vk::ImageAspectFlags::COLOR))
            .collect();

        // ── Allocator ─────────────────────────────────────────────────────
        let mut allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device:   device.clone(),
            physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        }).unwrap();

        // ── Depth buffer ──────────────────────────────────────────────────
        let depth_format = vk::Format::D32_SFLOAT;
        let (depth_image, depth_view, depth_alloc) =
            create_depth(&device, &mut allocator, extent, depth_format);

        // ── Command pool ──────────────────────────────────────────────────
        let cmd_pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo {
                queue_family_index: graphics_family,
                flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
                ..Default::default()
            }, None).unwrap();
        let cmd_buffers = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo {
                command_pool: cmd_pool,
                level: vk::CommandBufferLevel::PRIMARY,
                command_buffer_count: FRAMES_IN_FLIGHT as u32,
                ..Default::default()
            }).unwrap();

        // ── Render pass ───────────────────────────────────────────────────
        let render_pass = make_render_pass(&device, sc_format, depth_format);

        // ── Framebuffers ──────────────────────────────────────────────────
        let framebuffers: Vec<_> = sc_views
            .iter()
            .map(|&cv| {
                let atts = [cv, depth_view];
                device.create_framebuffer(&vk::FramebufferCreateInfo {
                    render_pass,
                    attachment_count: atts.len() as u32,
                    p_attachments: atts.as_ptr(),
                    width: extent.width,
                    height: extent.height,
                    layers: 1,
                    ..Default::default()
                }, None).unwrap()
            })
            .collect();

        // ── Descriptor layouts ────────────────────────────────────────────
        let desc_layout_3d = {
            let b = vk::DescriptorSetLayoutBinding {
                binding: 0,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                ..Default::default()
            };
            device.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo {
                binding_count: 1, p_bindings: &b, ..Default::default()
            }, None).unwrap()
        };
        let desc_layout_hud = {
            let bs = [
                vk::DescriptorSetLayoutBinding {
                    binding: 0,
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                    stage_flags: vk::ShaderStageFlags::VERTEX,
                    ..Default::default()
                },
                vk::DescriptorSetLayoutBinding {
                    binding: 1,
                    descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                    ..Default::default()
                },
            ];
            device.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo {
                binding_count: bs.len() as u32, p_bindings: bs.as_ptr(), ..Default::default()
            }, None).unwrap()
        };

        // ── Descriptor pool ───────────────────────────────────────────────
        let pool_sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::UNIFORM_BUFFER, descriptor_count: (FRAMES_IN_FLIGHT * 4) as u32 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: (FRAMES_IN_FLIGHT * 2) as u32 },
        ];
        let desc_pool = device.create_descriptor_pool(&vk::DescriptorPoolCreateInfo {
            max_sets: (FRAMES_IN_FLIGHT * 4) as u32,
            pool_size_count: pool_sizes.len() as u32,
            p_pool_sizes: pool_sizes.as_ptr(),
            ..Default::default()
        }, None).unwrap();

        let layouts_3d:  Vec<_> = (0..FRAMES_IN_FLIGHT).map(|_| desc_layout_3d).collect();
        let layouts_hud: Vec<_> = (0..FRAMES_IN_FLIGHT).map(|_| desc_layout_hud).collect();
        let desc_sets_3d = device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
            descriptor_pool: desc_pool,
            descriptor_set_count: FRAMES_IN_FLIGHT as u32,
            p_set_layouts: layouts_3d.as_ptr(),
            ..Default::default()
        }).unwrap();
        let desc_sets_hud = device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
            descriptor_pool: desc_pool,
            descriptor_set_count: FRAMES_IN_FLIGHT as u32,
            p_set_layouts: layouts_hud.as_ptr(),
            ..Default::default()
        }).unwrap();

        // ── UBO buffers ───────────────────────────────────────────────────
        let ubo3d_sz   = std::mem::size_of::<UBO3D>() as u64;
        let ubohud_sz  = std::mem::size_of::<UBOHUD>() as u64;
        let mut ubo3d_bufs    = Vec::new();
        let mut ubo3d_allocs  = Vec::new();
        let mut ubohud_bufs   = Vec::new();
        let mut ubohud_allocs = Vec::new();

        for _ in 0..FRAMES_IN_FLIGHT {
            let (b, a) = buffer::create_mapped_buffer(&device, &mut allocator, ubo3d_sz,  vk::BufferUsageFlags::UNIFORM_BUFFER, "ubo3d");
            ubo3d_bufs.push(b); ubo3d_allocs.push(Some(a));
            let (b, a) = buffer::create_mapped_buffer(&device, &mut allocator, ubohud_sz, vk::BufferUsageFlags::UNIFORM_BUFFER, "ubohud");
            ubohud_bufs.push(b); ubohud_allocs.push(Some(a));
        }

        // Write UBO bindings into descriptor sets
        for i in 0..FRAMES_IN_FLIGHT {
            let bi3d  = vk::DescriptorBufferInfo { buffer: ubo3d_bufs[i],   offset: 0, range: ubo3d_sz };
            let bihud = vk::DescriptorBufferInfo { buffer: ubohud_bufs[i],  offset: 0, range: ubohud_sz };
            device.update_descriptor_sets(&[
                vk::WriteDescriptorSet { dst_set: desc_sets_3d[i],  dst_binding: 0, descriptor_count: 1, descriptor_type: vk::DescriptorType::UNIFORM_BUFFER, p_buffer_info: &bi3d,  ..Default::default() },
                vk::WriteDescriptorSet { dst_set: desc_sets_hud[i], dst_binding: 0, descriptor_count: 1, descriptor_type: vk::DescriptorType::UNIFORM_BUFFER, p_buffer_info: &bihud, ..Default::default() },
            ], &[]);
        }

        // ── Sync primitives ───────────────────────────────────────────────
        let mut img_available = Vec::new();
        let mut render_done   = Vec::new();
        let mut in_flight     = Vec::new();
        for _ in 0..FRAMES_IN_FLIGHT {
            img_available.push(device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).unwrap());
            render_done.push(  device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).unwrap());
            in_flight.push(    device.create_fence(&vk::FenceCreateInfo { flags: vk::FenceCreateFlags::SIGNALED, ..Default::default() }, None).unwrap());
        }

        // ── Pipelines ─────────────────────────────────────────────────────
        let (scene_layout, scene_pipeline) = create_scene_pipeline(&device, render_pass, desc_layout_3d);
        let (trail_layout, trail_pipeline) = create_trail_pipeline(&device, render_pass, desc_layout_3d);
        let (hud_layout,   hud_pipeline)   = create_hud_pipeline(&device, render_pass, desc_layout_hud);

        VulkanContext {
            entry, instance, surface_loader, surface,
            physical_device, device, graphics_queue, present_queue,
            graphics_family, present_family,
            swapchain_loader: sc_loader, swapchain, swapchain_images: sc_images,
            swapchain_views: sc_views, swapchain_format: sc_format, extent,
            depth_image, depth_view, depth_alloc: Some(depth_alloc),
            render_pass, framebuffers,
            cmd_pool, cmd_buffers, allocator,
            desc_pool, desc_layout_3d, desc_layout_hud, desc_sets_3d, desc_sets_hud,
            ubo3d_bufs, ubo3d_allocs, ubohud_bufs, ubohud_allocs,
            scene_pipeline, scene_layout, trail_pipeline, trail_layout,
            hud_pipeline, hud_layout,
            img_available, render_done, in_flight,
            current_frame: 0,
            last_image_idx: 0,
        }
    }

    // ── Frame management ──────────────────────────────────────────────────

    pub fn begin_frame(&mut self) -> (vk::CommandBuffer, u32, usize) {
        unsafe {
            let f = self.current_frame;
            self.device.wait_for_fences(&[self.in_flight[f]], true, u64::MAX).unwrap();

            let (image_idx, _) = self.swapchain_loader
                .acquire_next_image(self.swapchain, u64::MAX, self.img_available[f], vk::Fence::null())
                .unwrap();

            self.device.reset_fences(&[self.in_flight[f]]).unwrap();

            let cmd = self.cmd_buffers[f];
            self.device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty()).unwrap();
            self.device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default()).unwrap();

            let clears = [
                vk::ClearValue { color: vk::ClearColorValue { float32: [0.18, 0.46, 0.88, 1.0] } },
                vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } },
            ];
            self.device.cmd_begin_render_pass(cmd, &vk::RenderPassBeginInfo {
                render_pass: self.render_pass,
                framebuffer: self.framebuffers[image_idx as usize],
                render_area: vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: self.extent },
                clear_value_count: clears.len() as u32,
                p_clear_values: clears.as_ptr(),
                ..Default::default()
            }, vk::SubpassContents::INLINE);

            self.device.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                x: 0.0, y: 0.0,
                width: self.extent.width as f32, height: self.extent.height as f32,
                min_depth: 0.0, max_depth: 1.0,
            }]);
            self.device.cmd_set_scissor(cmd, 0, &[vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 }, extent: self.extent,
            }]);

            (cmd, image_idx, f)
        }
    }

    pub fn end_frame(&mut self, cmd: vk::CommandBuffer, image_idx: u32) {
        unsafe {
            let f = self.current_frame;
            self.device.cmd_end_render_pass(cmd);
            self.device.end_command_buffer(cmd).unwrap();

            let wait   = [self.img_available[f]];
            let signal = [self.render_done[f]];
            let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            self.device.queue_submit(self.graphics_queue, &[vk::SubmitInfo {
                wait_semaphore_count: 1,    p_wait_semaphores: wait.as_ptr(),
                p_wait_dst_stage_mask: stages.as_ptr(),
                command_buffer_count: 1,    p_command_buffers: &cmd,
                signal_semaphore_count: 1,  p_signal_semaphores: signal.as_ptr(),
                ..Default::default()
            }], self.in_flight[f]).unwrap();

            let scs = [self.swapchain];
            let idxs = [image_idx];
            let _ = self.swapchain_loader.queue_present(self.present_queue, &vk::PresentInfoKHR {
                wait_semaphore_count: 1, p_wait_semaphores: signal.as_ptr(),
                swapchain_count: 1,      p_swapchains: scs.as_ptr(),
                p_image_indices: idxs.as_ptr(),
                ..Default::default()
            });

            self.last_image_idx = image_idx;
            self.current_frame = (f + 1) % FRAMES_IN_FLIGHT;
        }
    }

    // ── UBO update helpers ────────────────────────────────────────────────

    pub fn update_ubo3d(&mut self, frame: usize, data: &UBO3D) {
        write_ubo(self.ubo3d_allocs[frame].as_mut().unwrap(), data);
    }

    pub fn update_ubohud(&mut self, frame: usize, data: &UBOHUD) {
        write_ubo(self.ubohud_allocs[frame].as_mut().unwrap(), data);
    }

    pub fn set_font_texture(&mut self, image_view: vk::ImageView, sampler: vk::Sampler) {
        for i in 0..FRAMES_IN_FLIGHT {
            let ii = vk::DescriptorImageInfo {
                sampler,
                image_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            };
            unsafe {
                self.device.update_descriptor_sets(&[vk::WriteDescriptorSet {
                    dst_set: self.desc_sets_hud[i],
                    dst_binding: 1,
                    descriptor_count: 1,
                    descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    p_image_info: &ii,
                    ..Default::default()
                }], &[]);
            }
        }
    }

    pub fn wait_idle(&self) {
        unsafe { self.device.device_wait_idle().unwrap() };
    }

    /// Capture the last-presented swapchain image and write it as a PNG.
    /// Must be called right after render() while no other frame is in flight.
    pub fn save_screenshot(&mut self, path: &str) {
        use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};
        use gpu_allocator::MemoryLocation;

        unsafe {
            self.device.device_wait_idle().unwrap();

            let w = self.extent.width;
            let h = self.extent.height;
            let buf_size = (w * h * 4) as u64;

            // Create host-visible readback buffer
            let buf_info = vk::BufferCreateInfo {
                size: buf_size,
                usage: vk::BufferUsageFlags::TRANSFER_DST,
                sharing_mode: vk::SharingMode::EXCLUSIVE,
                ..Default::default()
            };
            let readback_buf = self.device.create_buffer(&buf_info, None).unwrap();
            let req = self.device.get_buffer_memory_requirements(readback_buf);
            let readback_alloc = self.allocator.allocate(&AllocationCreateDesc {
                name: "screenshot_readback",
                requirements: req,
                location: MemoryLocation::GpuToCpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            }).unwrap();
            self.device.bind_buffer_memory(readback_buf, readback_alloc.memory(), readback_alloc.offset()).unwrap();

            let image = self.swapchain_images[self.last_image_idx as usize];

            let cmd = buffer::begin_one_shot(&self.device, self.cmd_pool);

            // Transition: PRESENT_SRC_KHR → TRANSFER_SRC_OPTIMAL
            let barrier_to_src = vk::ImageMemoryBarrier {
                src_access_mask: vk::AccessFlags::MEMORY_READ,
                dst_access_mask: vk::AccessFlags::TRANSFER_READ,
                old_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                new_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                image,
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0, level_count: 1,
                    base_array_layer: 0, layer_count: 1,
                },
                ..Default::default()
            };
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[], &[], &[barrier_to_src],
            );

            // Copy image → buffer
            let region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D { width: w, height: h, depth: 1 },
            };
            self.device.cmd_copy_image_to_buffer(
                cmd, image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, readback_buf, &[region],
            );

            buffer::end_one_shot(&self.device, self.graphics_queue, self.cmd_pool, cmd);

            // BGRA → RGBA and write PNG
            let raw = readback_alloc.mapped_slice().unwrap();
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            for i in 0..(w * h) as usize {
                rgba[i * 4]     = raw[i * 4 + 2]; // R ← B
                rgba[i * 4 + 1] = raw[i * 4 + 1]; // G
                rgba[i * 4 + 2] = raw[i * 4];     // B ← R
                rgba[i * 4 + 3] = raw[i * 4 + 3]; // A
            }

            let file = std::fs::File::create(path).expect("Cannot create screenshot file");
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
            drop(writer);

            self.allocator.free(readback_alloc).unwrap();
            self.device.destroy_buffer(readback_buf, None);

            println!("Screenshot saved to {}", path);
        }
    }
}

fn write_ubo<T>(alloc: &mut Allocation, data: &T) {
    let bytes = unsafe {
        std::slice::from_raw_parts((data as *const T) as *const u8, std::mem::size_of::<T>())
    };
    alloc.mapped_slice_mut().unwrap()[..bytes.len()].copy_from_slice(bytes);
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            for &p in &[self.scene_pipeline, self.trail_pipeline, self.hud_pipeline] {
                self.device.destroy_pipeline(p, None);
            }
            for &l in &[self.scene_layout, self.trail_layout, self.hud_layout] {
                self.device.destroy_pipeline_layout(l, None);
            }
            for i in 0..FRAMES_IN_FLIGHT {
                if let Some(a) = self.ubo3d_allocs[i].take()  { let _ = self.allocator.free(a); }
                if let Some(a) = self.ubohud_allocs[i].take() { let _ = self.allocator.free(a); }
                self.device.destroy_buffer(self.ubo3d_bufs[i], None);
                self.device.destroy_buffer(self.ubohud_bufs[i], None);
                self.device.destroy_semaphore(self.img_available[i], None);
                self.device.destroy_semaphore(self.render_done[i], None);
                self.device.destroy_fence(self.in_flight[i], None);
            }
            self.device.destroy_descriptor_pool(self.desc_pool, None);
            self.device.destroy_descriptor_set_layout(self.desc_layout_3d, None);
            self.device.destroy_descriptor_set_layout(self.desc_layout_hud, None);
            for &fb in &self.framebuffers { self.device.destroy_framebuffer(fb, None); }
            self.device.destroy_render_pass(self.render_pass, None);
            self.device.destroy_command_pool(self.cmd_pool, None);
            self.device.destroy_image_view(self.depth_view, None);
            if let Some(a) = self.depth_alloc.take() { let _ = self.allocator.free(a); }
            self.device.destroy_image(self.depth_image, None);
            for &v in &self.swapchain_views { self.device.destroy_image_view(v, None); }
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

// ── Private helpers ───────────────────────────────────────────────────────

unsafe fn pick_device(
    instance: &ash::Instance,
    sl: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    pd: vk::PhysicalDevice,
) -> Option<(vk::PhysicalDevice, u32, u32)> {
    let qfp = instance.get_physical_device_queue_family_properties(pd);
    let mut gfx = None;
    let mut prs = None;
    for (i, p) in qfp.iter().enumerate() {
        if p.queue_flags.contains(vk::QueueFlags::GRAPHICS) { gfx = Some(i as u32); }
        if sl.get_physical_device_surface_support(pd, i as u32, surface).unwrap_or(false) {
            prs = Some(i as u32);
        }
    }
    let (g, p) = (gfx?, prs?);
    let exts = instance.enumerate_device_extension_properties(pd).unwrap_or_default();
    let ok = exts.iter().any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == ash::khr::swapchain::NAME);
    if !ok { return None; }
    Some((pd, g, p))
}

unsafe fn create_swapchain(
    instance: &ash::Instance,
    device: &ash::Device,
    sl: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    pd: vk::PhysicalDevice,
    gfx: u32, prs: u32,
    window: &Arc<Window>,
) -> (ash::khr::swapchain::Device, vk::SwapchainKHR, Vec<vk::Image>, vk::Format, vk::Extent2D) {
    let caps = sl.get_physical_device_surface_capabilities(pd, surface).unwrap();
    let fmts = sl.get_physical_device_surface_formats(pd, surface).unwrap();
    let modes = sl.get_physical_device_surface_present_modes(pd, surface).unwrap();

    let fmt = fmts.iter().find(|f| f.format == vk::Format::B8G8R8A8_SRGB && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR).cloned().unwrap_or(fmts[0]);
    let mode = modes.iter().find(|&&m| m == vk::PresentModeKHR::MAILBOX).cloned().unwrap_or(vk::PresentModeKHR::FIFO);

    let sz = window.inner_size();
    let extent = if caps.current_extent.width != u32::MAX {
        caps.current_extent
    } else {
        vk::Extent2D {
            width:  sz.width .clamp(caps.min_image_extent.width,  caps.max_image_extent.width),
            height: sz.height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
        }
    };
    let img_count = (caps.min_image_count + 1).min(if caps.max_image_count > 0 { caps.max_image_count } else { u32::MAX });

    let (sharing, families): (vk::SharingMode, Vec<u32>) = if gfx != prs {
        (vk::SharingMode::CONCURRENT, vec![gfx, prs])
    } else {
        (vk::SharingMode::EXCLUSIVE, vec![])
    };

    let sc_loader = ash::khr::swapchain::Device::new(instance, device);
    let sc = sc_loader.create_swapchain(&vk::SwapchainCreateInfoKHR {
        surface,
        min_image_count: img_count,
        image_format: fmt.format,
        image_color_space: fmt.color_space,
        image_extent: extent,
        image_array_layers: 1,
        image_usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
        image_sharing_mode: sharing,
        queue_family_index_count: families.len() as u32,
        p_queue_family_indices: families.as_ptr(),
        pre_transform: caps.current_transform,
        composite_alpha: vk::CompositeAlphaFlagsKHR::OPAQUE,
        present_mode: mode,
        clipped: vk::TRUE,
        ..Default::default()
    }, None).unwrap();

    let images = sc_loader.get_swapchain_images(sc).unwrap();
    (sc_loader, sc, images, fmt.format, extent)
}

fn make_image_view(device: &ash::Device, image: vk::Image, format: vk::Format, aspect: vk::ImageAspectFlags) -> vk::ImageView {
    unsafe {
        device.create_image_view(&vk::ImageViewCreateInfo {
            image, view_type: vk::ImageViewType::TYPE_2D, format,
            components: vk::ComponentMapping::default(),
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: aspect, base_mip_level: 0, level_count: 1,
                base_array_layer: 0, layer_count: 1,
            },
            ..Default::default()
        }, None).unwrap()
    }
}

fn create_depth(device: &ash::Device, allocator: &mut Allocator, extent: vk::Extent2D, format: vk::Format) -> (vk::Image, vk::ImageView, Allocation) {
    let image = unsafe { device.create_image(&vk::ImageCreateInfo {
        image_type: vk::ImageType::TYPE_2D, format,
        extent: vk::Extent3D { width: extent.width, height: extent.height, depth: 1 },
        mip_levels: 1, array_layers: 1,
        samples: vk::SampleCountFlags::TYPE_1,
        tiling: vk::ImageTiling::OPTIMAL,
        usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        sharing_mode: vk::SharingMode::EXCLUSIVE,
        initial_layout: vk::ImageLayout::UNDEFINED,
        ..Default::default()
    }, None).unwrap() };
    let req = unsafe { device.get_image_memory_requirements(image) };
    let alloc = allocator.allocate(&AllocationCreateDesc {
        name: "depth", requirements: req,
        location: MemoryLocation::GpuOnly, linear: false,
        allocation_scheme: AllocationScheme::GpuAllocatorManaged,
    }).unwrap();
    unsafe { device.bind_image_memory(image, alloc.memory(), alloc.offset()).unwrap() };
    let view = make_image_view(device, image, format, vk::ImageAspectFlags::DEPTH);
    (image, view, alloc)
}

fn make_render_pass(device: &ash::Device, color: vk::Format, depth: vk::Format) -> vk::RenderPass {
    let atts = [
        vk::AttachmentDescription {
            format: color, samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR, store_op: vk::AttachmentStoreOp::STORE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            ..Default::default()
        },
        vk::AttachmentDescription {
            format: depth, samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR, store_op: vk::AttachmentStoreOp::DONT_CARE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            ..Default::default()
        },
    ];
    let color_ref = vk::AttachmentReference { attachment: 0, layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL };
    let depth_ref = vk::AttachmentReference { attachment: 1, layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL };
    let subpass = vk::SubpassDescription {
        pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
        color_attachment_count: 1, p_color_attachments: &color_ref,
        p_depth_stencil_attachment: &depth_ref,
        ..Default::default()
    };
    let dep = vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL, dst_subpass: 0,
        src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        src_access_mask: vk::AccessFlags::empty(),
        dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ..Default::default()
    };
    unsafe { device.create_render_pass(&vk::RenderPassCreateInfo {
        attachment_count: atts.len() as u32, p_attachments: atts.as_ptr(),
        subpass_count: 1, p_subpasses: &subpass,
        dependency_count: 1, p_dependencies: &dep,
        ..Default::default()
    }, None).unwrap() }
}

// ── Pipeline helpers ──────────────────────────────────────────────────────

fn make_shader_mod(device: &ash::Device, spv: &[u8]) -> vk::ShaderModule {
    let code: Vec<u32> = spv.chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    unsafe { device.create_shader_module(&vk::ShaderModuleCreateInfo {
        code_size: spv.len(), p_code: code.as_ptr(), ..Default::default()
    }, None).unwrap() }
}

fn main_name() -> &'static CStr {
    unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") }
}

unsafe fn create_scene_pipeline(device: &ash::Device, rp: vk::RenderPass, layout_set: vk::DescriptorSetLayout) -> (vk::PipelineLayout, vk::Pipeline) {
    let vert = make_shader_mod(device, SCENE_VERT);
    let frag = make_shader_mod(device, SCENE_FRAG);
    let stages = shader_stages(vert, frag);
    let bind = vk::VertexInputBindingDescription { binding: 0, stride: 24, input_rate: vk::VertexInputRate::VERTEX };
    let attrs = [
        vk::VertexInputAttributeDescription { location: 0, binding: 0, format: vk::Format::R32G32B32_SFLOAT, offset: 0 },
        vk::VertexInputAttributeDescription { location: 1, binding: 0, format: vk::Format::R32G32B32_SFLOAT, offset: 12 },
    ];
    let vi = vi_state(&bind, &attrs);
    let push = vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, offset: 0, size: std::mem::size_of::<Push3D>() as u32 };
    let layout = device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo { set_layout_count: 1, p_set_layouts: &layout_set, push_constant_range_count: 1, p_push_constant_ranges: &push, ..Default::default() }, None).unwrap();
    let blend = opaque_blend();
    let ds = depth_state(true, true);
    let pl = make_gfx_pipeline(device, rp, layout, &stages, &vi, &blend, &ds, vk::CullModeFlags::BACK);
    device.destroy_shader_module(vert, None);
    device.destroy_shader_module(frag, None);
    (layout, pl)
}

unsafe fn create_trail_pipeline(device: &ash::Device, rp: vk::RenderPass, layout_set: vk::DescriptorSetLayout) -> (vk::PipelineLayout, vk::Pipeline) {
    let vert = make_shader_mod(device, TRAIL_VERT);
    let frag = make_shader_mod(device, TRAIL_FRAG);
    let stages = shader_stages(vert, frag);
    let bind = vk::VertexInputBindingDescription { binding: 0, stride: 28, input_rate: vk::VertexInputRate::VERTEX };
    let attrs = [
        vk::VertexInputAttributeDescription { location: 0, binding: 0, format: vk::Format::R32G32B32_SFLOAT,   offset: 0 },
        vk::VertexInputAttributeDescription { location: 1, binding: 0, format: vk::Format::R32G32B32A32_SFLOAT, offset: 12 },
    ];
    let vi = vi_state(&bind, &attrs);
    let push = vk::PushConstantRange { stage_flags: vk::ShaderStageFlags::VERTEX, offset: 0, size: std::mem::size_of::<PushTrail>() as u32 };
    let layout = device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo { set_layout_count: 1, p_set_layouts: &layout_set, push_constant_range_count: 1, p_push_constant_ranges: &push, ..Default::default() }, None).unwrap();
    let blend = alpha_blend();
    let ds = depth_state(true, false);
    let pl = make_gfx_pipeline(device, rp, layout, &stages, &vi, &blend, &ds, vk::CullModeFlags::NONE);
    device.destroy_shader_module(vert, None);
    device.destroy_shader_module(frag, None);
    (layout, pl)
}

unsafe fn create_hud_pipeline(device: &ash::Device, rp: vk::RenderPass, layout_set: vk::DescriptorSetLayout) -> (vk::PipelineLayout, vk::Pipeline) {
    let vert = make_shader_mod(device, HUD_VERT);
    let frag = make_shader_mod(device, HUD_FRAG);
    let stages = shader_stages(vert, frag);
    let bind = vk::VertexInputBindingDescription { binding: 0, stride: 36, input_rate: vk::VertexInputRate::VERTEX };
    let attrs = [
        vk::VertexInputAttributeDescription { location: 0, binding: 0, format: vk::Format::R32G32_SFLOAT,       offset: 0 },
        vk::VertexInputAttributeDescription { location: 1, binding: 0, format: vk::Format::R32G32_SFLOAT,       offset: 8 },
        vk::VertexInputAttributeDescription { location: 2, binding: 0, format: vk::Format::R32G32B32A32_SFLOAT,  offset: 16 },
        vk::VertexInputAttributeDescription { location: 3, binding: 0, format: vk::Format::R32_SFLOAT,           offset: 32 },
    ];
    let vi = vi_state(&bind, &attrs);
    let layout = device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo { set_layout_count: 1, p_set_layouts: &layout_set, ..Default::default() }, None).unwrap();
    let blend = alpha_blend();
    let ds = depth_state(false, false);
    let pl = make_gfx_pipeline(device, rp, layout, &stages, &vi, &blend, &ds, vk::CullModeFlags::NONE);
    device.destroy_shader_module(vert, None);
    device.destroy_shader_module(frag, None);
    (layout, pl)
}

fn shader_stages(vert: vk::ShaderModule, frag: vk::ShaderModule) -> [vk::PipelineShaderStageCreateInfo<'static>; 2] {
    [
        vk::PipelineShaderStageCreateInfo { stage: vk::ShaderStageFlags::VERTEX,   module: vert, p_name: main_name().as_ptr(), ..Default::default() },
        vk::PipelineShaderStageCreateInfo { stage: vk::ShaderStageFlags::FRAGMENT, module: frag, p_name: main_name().as_ptr(), ..Default::default() },
    ]
}

fn vi_state<'a>(bind: &'a vk::VertexInputBindingDescription, attrs: &'a [vk::VertexInputAttributeDescription]) -> vk::PipelineVertexInputStateCreateInfo<'a> {
    vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: 1,    p_vertex_binding_descriptions: bind,
        vertex_attribute_description_count: attrs.len() as u32, p_vertex_attribute_descriptions: attrs.as_ptr(),
        ..Default::default()
    }
}

fn opaque_blend() -> vk::PipelineColorBlendAttachmentState {
    vk::PipelineColorBlendAttachmentState { blend_enable: vk::FALSE, color_write_mask: vk::ColorComponentFlags::RGBA, ..Default::default() }
}

fn alpha_blend() -> vk::PipelineColorBlendAttachmentState {
    vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE,
        src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
        dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        color_blend_op: vk::BlendOp::ADD,
        src_alpha_blend_factor: vk::BlendFactor::ONE,
        dst_alpha_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        alpha_blend_op: vk::BlendOp::ADD,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    }
}

fn depth_state(test: bool, write: bool) -> vk::PipelineDepthStencilStateCreateInfo<'static> {
    vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable:  if test  { vk::TRUE } else { vk::FALSE },
        depth_write_enable: if write { vk::TRUE } else { vk::FALSE },
        depth_compare_op: if test { vk::CompareOp::LESS } else { vk::CompareOp::ALWAYS },
        ..Default::default()
    }
}

unsafe fn make_gfx_pipeline(
    device: &ash::Device,
    rp: vk::RenderPass,
    layout: vk::PipelineLayout,
    stages: &[vk::PipelineShaderStageCreateInfo<'_>],
    vi: &vk::PipelineVertexInputStateCreateInfo<'_>,
    blend_att: &vk::PipelineColorBlendAttachmentState,
    ds: &vk::PipelineDepthStencilStateCreateInfo<'_>,
    cull: vk::CullModeFlags,
) -> vk::Pipeline {
    let ia = vk::PipelineInputAssemblyStateCreateInfo { topology: vk::PrimitiveTopology::TRIANGLE_LIST, primitive_restart_enable: vk::FALSE, ..Default::default() };
    let vp = vk::PipelineViewportStateCreateInfo { viewport_count: 1, scissor_count: 1, ..Default::default() };
    // Y-flip in projection reverses apparent winding → use CLOCKWISE to cull back faces
    let rs = vk::PipelineRasterizationStateCreateInfo { polygon_mode: vk::PolygonMode::FILL, cull_mode: cull, front_face: vk::FrontFace::CLOCKWISE, line_width: 1.0, ..Default::default() };
    let ms = vk::PipelineMultisampleStateCreateInfo { rasterization_samples: vk::SampleCountFlags::TYPE_1, ..Default::default() };
    let cb = vk::PipelineColorBlendStateCreateInfo { attachment_count: 1, p_attachments: blend_att, ..Default::default() };
    let dyn_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dyn_info = vk::PipelineDynamicStateCreateInfo { dynamic_state_count: dyn_states.len() as u32, p_dynamic_states: dyn_states.as_ptr(), ..Default::default() };

    device.create_graphics_pipelines(vk::PipelineCache::null(), &[vk::GraphicsPipelineCreateInfo {
        stage_count: stages.len() as u32, p_stages: stages.as_ptr(),
        p_vertex_input_state: vi, p_input_assembly_state: &ia,
        p_viewport_state: &vp, p_rasterization_state: &rs,
        p_multisample_state: &ms, p_depth_stencil_state: ds,
        p_color_blend_state: &cb, p_dynamic_state: &dyn_info,
        layout, render_pass: rp, subpass: 0,
        ..Default::default()
    }], None).unwrap()[0]
}
