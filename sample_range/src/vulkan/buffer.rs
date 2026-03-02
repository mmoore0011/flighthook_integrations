use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use gpu_allocator::MemoryLocation;

/// Upload `data` to a device-local buffer via a temporary staging buffer.
/// Returns the device-local (buffer, allocation).
pub unsafe fn upload_buffer(
    device: &ash::Device,
    allocator: &mut Allocator,
    queue: vk::Queue,
    cmd_pool: vk::CommandPool,
    data: &[u8],
    usage: vk::BufferUsageFlags,
) -> (vk::Buffer, Allocation) {
    let size = data.len() as u64;

    // ── Staging buffer (CPU→GPU) ──────────────────────────────────────────
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
            name: "staging",
            requirements: stage_req,
            location: MemoryLocation::CpuToGpu,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .unwrap();

    device
        .bind_buffer_memory(stage_buf, stage_alloc.memory(), stage_alloc.offset())
        .unwrap();

    stage_alloc
        .mapped_slice_mut()
        .expect("staging not mapped")[..data.len()]
        .copy_from_slice(data);

    // ── Device-local destination buffer ──────────────────────────────────
    let dst_info = vk::BufferCreateInfo {
        size,
        usage: usage | vk::BufferUsageFlags::TRANSFER_DST,
        sharing_mode: vk::SharingMode::EXCLUSIVE,
        ..Default::default()
    };
    let dst_buf = device.create_buffer(&dst_info, None).unwrap();
    let dst_req = device.get_buffer_memory_requirements(dst_buf);

    let dst_alloc = allocator
        .allocate(&AllocationCreateDesc {
            name: "device_local_buf",
            requirements: dst_req,
            location: MemoryLocation::GpuOnly,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .unwrap();

    device
        .bind_buffer_memory(dst_buf, dst_alloc.memory(), dst_alloc.offset())
        .unwrap();

    // ── One-shot copy command ─────────────────────────────────────────────
    let cmd = begin_one_shot(device, cmd_pool);
    let region = vk::BufferCopy { src_offset: 0, dst_offset: 0, size };
    device.cmd_copy_buffer(cmd, stage_buf, dst_buf, &[region]);
    end_one_shot(device, queue, cmd_pool, cmd);

    // ── Free staging ──────────────────────────────────────────────────────
    allocator.free(stage_alloc).unwrap();
    device.destroy_buffer(stage_buf, None);

    (dst_buf, dst_alloc)
}

/// Create a persistently-mapped host-coherent buffer (for UBOs, trail, HUD).
/// Returns (buffer, allocation, mapped_ptr).
pub unsafe fn create_mapped_buffer(
    device: &ash::Device,
    allocator: &mut Allocator,
    size: u64,
    usage: vk::BufferUsageFlags,
    name: &str,
) -> (vk::Buffer, Allocation) {
    let buf_info = vk::BufferCreateInfo {
        size,
        usage,
        sharing_mode: vk::SharingMode::EXCLUSIVE,
        ..Default::default()
    };
    let buf = device.create_buffer(&buf_info, None).unwrap();
    let req = device.get_buffer_memory_requirements(buf);

    let alloc = allocator
        .allocate(&AllocationCreateDesc {
            name,
            requirements: req,
            location: MemoryLocation::CpuToGpu,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .unwrap();

    device.bind_buffer_memory(buf, alloc.memory(), alloc.offset()).unwrap();

    (buf, alloc)
}

// ── One-shot command helpers ──────────────────────────────────────────────

pub unsafe fn begin_one_shot(device: &ash::Device, pool: vk::CommandPool) -> vk::CommandBuffer {
    let alloc_info = vk::CommandBufferAllocateInfo {
        command_pool: pool,
        level: vk::CommandBufferLevel::PRIMARY,
        command_buffer_count: 1,
        ..Default::default()
    };
    let cmd = device.allocate_command_buffers(&alloc_info).unwrap()[0];
    let begin = vk::CommandBufferBeginInfo {
        flags: vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
        ..Default::default()
    };
    device.begin_command_buffer(cmd, &begin).unwrap();
    cmd
}

pub unsafe fn end_one_shot(
    device: &ash::Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
) {
    device.end_command_buffer(cmd).unwrap();
    let submit = vk::SubmitInfo {
        command_buffer_count: 1,
        p_command_buffers: &cmd,
        ..Default::default()
    };
    device.queue_submit(queue, &[submit], vk::Fence::null()).unwrap();
    device.queue_wait_idle(queue).unwrap();
    device.free_command_buffers(pool, &[cmd]);
}
