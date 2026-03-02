use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use gpu_allocator::MemoryLocation;

use super::buffer::{begin_one_shot, end_one_shot};

/// Upload R8_UNORM pixel data to a device-local VkImage.
/// Returns (image, image_view, allocation).
pub unsafe fn upload_r8_texture(
    device: &ash::Device,
    allocator: &mut Allocator,
    queue: vk::Queue,
    cmd_pool: vk::CommandPool,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> (vk::Image, vk::ImageView, Allocation) {
    let size = pixels.len() as u64;

    // ── Staging buffer ───────────────────────────────────────────────────
    let stage_info = vk::BufferCreateInfo {
        size,
        usage: vk::BufferUsageFlags::TRANSFER_SRC,
        sharing_mode: vk::SharingMode::EXCLUSIVE,
        ..Default::default()
    };
    let stage_buf = device.create_buffer(&stage_info, None).unwrap();
    let stage_req = device.get_buffer_memory_requirements(stage_buf);
    let mut stage_alloc = allocator
        .allocate(&AllocationCreateDesc {
            name: "font_stage",
            requirements: stage_req,
            location: MemoryLocation::CpuToGpu,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .unwrap();
    device
        .bind_buffer_memory(stage_buf, stage_alloc.memory(), stage_alloc.offset())
        .unwrap();
    stage_alloc.mapped_slice_mut().unwrap()[..pixels.len()].copy_from_slice(pixels);

    // ── Device image ─────────────────────────────────────────────────────
    let img_info = vk::ImageCreateInfo {
        image_type: vk::ImageType::TYPE_2D,
        format: vk::Format::R8_UNORM,
        extent: vk::Extent3D { width, height, depth: 1 },
        mip_levels: 1,
        array_layers: 1,
        samples: vk::SampleCountFlags::TYPE_1,
        tiling: vk::ImageTiling::OPTIMAL,
        usage: vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        sharing_mode: vk::SharingMode::EXCLUSIVE,
        initial_layout: vk::ImageLayout::UNDEFINED,
        ..Default::default()
    };
    let image = device.create_image(&img_info, None).unwrap();
    let img_req = device.get_image_memory_requirements(image);
    let img_alloc = allocator
        .allocate(&AllocationCreateDesc {
            name: "font_atlas",
            requirements: img_req,
            location: MemoryLocation::GpuOnly,
            linear: false,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .unwrap();
    device
        .bind_image_memory(image, img_alloc.memory(), img_alloc.offset())
        .unwrap();

    // ── Copy: UNDEFINED → TRANSFER_DST → SHADER_READ ────────────────────
    let cmd = begin_one_shot(device, cmd_pool);

    // Barrier: UNDEFINED → TRANSFER_DST
    transition_image_layout(
        device, cmd, image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    );

    // Buffer → Image copy
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
        image_extent: vk::Extent3D { width, height, depth: 1 },
    };
    device.cmd_copy_buffer_to_image(
        cmd,
        stage_buf,
        image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &[region],
    );

    // Barrier: TRANSFER_DST → SHADER_READ
    transition_image_layout(
        device, cmd, image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    );

    end_one_shot(device, queue, cmd_pool, cmd);

    // Free staging
    allocator.free(stage_alloc).unwrap();
    device.destroy_buffer(stage_buf, None);

    // ── Image view ───────────────────────────────────────────────────────
    let view_info = vk::ImageViewCreateInfo {
        image,
        view_type: vk::ImageViewType::TYPE_2D,
        format: vk::Format::R8_UNORM,
        components: vk::ComponentMapping::default(),
        subresource_range: vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        },
        ..Default::default()
    };
    let view = device.create_image_view(&view_info, None).unwrap();

    (image, view, img_alloc)
}

unsafe fn transition_image_layout(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) {
    let (src_access, dst_access, src_stage, dst_stage) = match (old_layout, new_layout) {
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
        ),
        (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
        ),
        _ => panic!("unsupported layout transition"),
    };

    let barrier = vk::ImageMemoryBarrier {
        old_layout,
        new_layout,
        src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
        dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
        image,
        subresource_range: vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        },
        src_access_mask: src_access,
        dst_access_mask: dst_access,
        ..Default::default()
    };

    device.cmd_pipeline_barrier(
        cmd,
        src_stage,
        dst_stage,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[barrier],
    );
}
