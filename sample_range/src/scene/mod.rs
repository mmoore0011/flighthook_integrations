pub mod meshes;

use ash::vk;
use glam::{Mat4, Vec3, Vec4};
use gpu_allocator::vulkan::Allocation;

use crate::vulkan::{buffer, VulkanContext, Push3D, PushTrail};
use meshes::{MeshData, Vertex3D};

// ── VertexTrail ───────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct VertexTrail {
    pub pos:   [f32; 3],
    pub color: [f32; 4],
}

// ── GPU mesh handle ───────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct GpuMesh {
    pub vbuf:        vk::Buffer,
    pub vbuf_alloc:  Allocation,
    pub ibuf:        vk::Buffer,
    pub ibuf_alloc:  Allocation,
    pub index_count: u32,
}

impl GpuMesh {
    pub fn upload(ctx: &mut VulkanContext, mesh: &MeshData) -> Self {
        let vbytes = unsafe {
            std::slice::from_raw_parts(
                mesh.vertices.as_ptr() as *const u8,
                mesh.vertices.len() * std::mem::size_of::<Vertex3D>(),
            )
        };
        let ibytes = unsafe {
            std::slice::from_raw_parts(
                mesh.indices.as_ptr() as *const u8,
                mesh.indices.len() * 4,
            )
        };
        let (vbuf, vbuf_alloc) = unsafe {
            buffer::upload_buffer(
                &ctx.device,
                &mut ctx.allocator,
                ctx.graphics_queue,
                ctx.cmd_pool,
                vbytes,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            )
        };
        let (ibuf, ibuf_alloc) = unsafe {
            buffer::upload_buffer(
                &ctx.device,
                &mut ctx.allocator,
                ctx.graphics_queue,
                ctx.cmd_pool,
                ibytes,
                vk::BufferUsageFlags::INDEX_BUFFER,
            )
        };
        GpuMesh {
            vbuf,
            vbuf_alloc,
            ibuf,
            ibuf_alloc,
            index_count: mesh.indices.len() as u32,
        }
    }

    #[allow(dead_code)]
    pub fn destroy(&mut self, ctx: &mut VulkanContext) {
        unsafe {
            ctx.device.destroy_buffer(self.vbuf, None);
            ctx.device.destroy_buffer(self.ibuf, None);
        }
        // Allocations freed when ctx.allocator drops
    }
}

// ── Pre-allocated host-coherent buffer for dynamic geometry ──────────────

pub struct DynBuffer {
    pub buf:   vk::Buffer,
    pub alloc: Allocation,
    pub cap:   usize,
}

impl DynBuffer {
    pub fn new(ctx: &mut VulkanContext, cap_bytes: usize, usage: vk::BufferUsageFlags) -> Self {
        let (buf, alloc) = unsafe {
            buffer::create_mapped_buffer(
                &ctx.device,
                &mut ctx.allocator,
                cap_bytes as u64,
                usage,
                "dyn_buf",
            )
        };
        DynBuffer { buf, alloc, cap: cap_bytes }
    }

    pub fn upload<T: Copy>(&mut self, data: &[T]) -> u32 {
        let bytes = unsafe {
            std::slice::from_raw_parts(
                data.as_ptr() as *const u8,
                data.len() * std::mem::size_of::<T>(),
            )
        };
        let len = bytes.len().min(self.cap);
        self.alloc.mapped_slice_mut().unwrap()[..len].copy_from_slice(&bytes[..len]);
        (len / std::mem::size_of::<T>()) as u32
    }
}

// ── Scene ─────────────────────────────────────────────────────────────────

pub struct Scene3D {
    pub ground:  GpuMesh,
    pub rings:   Vec<GpuMesh>,
    pub ball:    GpuMesh,
    pub trail:   DynBuffer,
    pub trail_vertex_count: u32,
}

const TRAIL_CAP: usize = 256 * 6 * std::mem::size_of::<VertexTrail>(); // 256 segments × 6 verts

impl Scene3D {
    pub fn new(ctx: &mut VulkanContext) -> Self {
        let ground_mesh = meshes::quad_xz(200.0, 350.0);
        let ground = GpuMesh::upload(ctx, &ground_mesh);

        let ring_mesh = meshes::cylinder(2.5, 0.04, 32);
        let rings = (0..4).map(|_| GpuMesh::upload(ctx, &ring_mesh)).collect();

        let ball_mesh = meshes::sphere(0.25, 16, 32);
        let ball = GpuMesh::upload(ctx, &ball_mesh);

        let trail = DynBuffer::new(ctx, TRAIL_CAP, vk::BufferUsageFlags::VERTEX_BUFFER);

        Scene3D { ground, rings, ball, trail, trail_vertex_count: 0 }
    }

    /// Draw scene + ball.  Trail drawn separately.
    pub fn draw_scene(
        &self,
        ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        frame: usize,
        ball_pos: Vec3,
    ) {
        unsafe {
            let dev = &ctx.device;
            dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, ctx.scene_pipeline);
            dev.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                ctx.scene_layout,
                0,
                &[ctx.desc_sets_3d[frame]],
                &[],
            );

            // Ground (200×350 yd quad centred at (0,0,−155))
            draw_mesh(
                dev, cmd, ctx.scene_layout, &self.ground,
                Mat4::from_translation(Vec3::new(0.0, 0.0, -155.0)),
                Vec4::new(0.18, 0.55, 0.10, 1.0),
                Vec4::ZERO,
            );

            // Distance rings at −50, −100, −150, −200
            let ring_dists = [50.0f32, 100.0, 150.0, 200.0];
            for (i, &d) in ring_dists.iter().enumerate() {
                draw_mesh(
                    dev, cmd, ctx.scene_layout, &self.rings[i],
                    Mat4::from_translation(Vec3::new(0.0, 0.03, -d)),
                    Vec4::new(1.0, 1.0, 1.0, 0.55),
                    Vec4::ZERO,
                );
            }

            // Ball (white + emission)
            draw_mesh(
                dev, cmd, ctx.scene_layout, &self.ball,
                Mat4::from_translation(ball_pos),
                Vec4::new(1.0, 1.0, 1.0, 1.0),
                Vec4::new(1.0, 1.0, 1.0, 0.6),
            );
        }
    }

    pub fn draw_trail(&self, ctx: &VulkanContext, cmd: vk::CommandBuffer, frame: usize) {
        if self.trail_vertex_count == 0 {
            return;
        }
        unsafe {
            let dev = &ctx.device;
            dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, ctx.trail_pipeline);
            dev.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                ctx.trail_layout,
                0,
                &[ctx.desc_sets_3d[frame]],
                &[],
            );

            // Trail push: identity model
            let push = PushTrail { model: Mat4::IDENTITY.to_cols_array_2d() };
            dev.cmd_push_constants(
                cmd,
                ctx.trail_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                std::slice::from_raw_parts(
                    (&push as *const PushTrail) as *const u8,
                    std::mem::size_of::<PushTrail>(),
                ),
            );

            dev.cmd_bind_vertex_buffers(cmd, 0, &[self.trail.buf], &[0]);
            dev.cmd_draw(cmd, self.trail_vertex_count, 1, 0, 0);
        }
    }

    /// Rebuild trail from a list of world-space points + camera position.
    pub fn rebuild_trail(&mut self, points: &[Vec3], cam_pos: Vec3) {
        const HALF_WIDTH: f32 = 0.18;
        let n = points.len();
        if n < 2 {
            self.trail_vertex_count = 0;
            return;
        }
        let mut verts: Vec<VertexTrail> = Vec::with_capacity((n - 1) * 6);
        for i in 0..n - 1 {
            let a = points[i];
            let b = points[i + 1];
            let t0 = i as f32 / (n - 1) as f32;
            let t1 = (i + 1) as f32 / (n - 1) as f32;

            let mut seg = b - a;
            if seg.length_squared() < 1e-8 {
                seg = Vec3::NEG_Z;
            }
            seg = seg.normalize();
            let mid = (a + b) * 0.5;
            let to_cam = (cam_pos - mid).normalize();
            let mut right = seg.cross(to_cam);
            if right.length_squared() < 1e-8 {
                right = Vec3::X;
            }
            right = right.normalize() * HALF_WIDTH;

            let c0 = trail_color(t0);
            let c1 = trail_color(t1);
            let al = a - right; let ar = a + right;
            let bl = b - right; let br = b + right;

            verts.push(VertexTrail { pos: al.into(), color: c0 });
            verts.push(VertexTrail { pos: bl.into(), color: c1 });
            verts.push(VertexTrail { pos: ar.into(), color: c0 });
            verts.push(VertexTrail { pos: ar.into(), color: c0 });
            verts.push(VertexTrail { pos: bl.into(), color: c1 });
            verts.push(VertexTrail { pos: br.into(), color: c1 });
        }
        self.trail_vertex_count = self.trail.upload(&verts);
    }
}

fn trail_color(t: f32) -> [f32; 4] {
    let alpha = t.powf(0.6) * 0.92;
    let g = 0.55 + 0.45 * t;
    let b = 0.15 + 0.80 * t;
    [1.0, g, b, alpha]
}

unsafe fn draw_mesh(
    dev: &ash::Device,
    cmd: vk::CommandBuffer,
    layout: vk::PipelineLayout,
    mesh: &GpuMesh,
    model: Mat4,
    color: Vec4,
    emission: Vec4,
) {
    let push = Push3D {
        model: model.to_cols_array_2d(),
        color: color.into(),
        emission: emission.into(),
    };
    dev.cmd_push_constants(
        cmd,
        layout,
        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        0,
        std::slice::from_raw_parts(
            (&push as *const Push3D) as *const u8,
            std::mem::size_of::<Push3D>(),
        ),
    );
    dev.cmd_bind_vertex_buffers(cmd, 0, &[mesh.vbuf], &[0]);
    dev.cmd_bind_index_buffer(cmd, mesh.ibuf, 0, vk::IndexType::UINT32);
    dev.cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, 0);
}
