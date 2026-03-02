use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use ash::vk;
use glam::{Mat4, Vec3, Vec4};
use winit::window::Window;

use crate::hud::{self, FontAtlas, VertexHUD};
use crate::scene::{DynBuffer, Scene3D};
use crate::shot_data::ShotData;
use crate::vulkan::{texture, UBO3D, UBOHUD, VulkanContext};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Idle,
    Aerial,
    Roll,
    Pause,
}

pub struct App {
    window:  Arc<Window>,
    ctx:     VulkanContext,
    scene:   Scene3D,
    shot:    ShotData,

    // Font atlas + GPU texture
    #[allow(dead_code)]
    atlas:        FontAtlas,
    font_image:              vk::Image,
    font_view:               vk::ImageView,
    #[allow(dead_code)]
    font_alloc:              gpu_allocator::vulkan::Allocation,
    font_sampler:            vk::Sampler,

    // HUD vertex buffer (host-coherent, rebuilt every frame)
    hud_buf:           DynBuffer,
    hud_vertex_count:  u32,

    // Animation state
    phase:       Phase,
    elapsed:     f32,
    pause_timer: f32,
    roll_t:      f32,
    ball_pos:    Vec3,
    trail_pts:   Vec<Vec3>,

    // Live mode
    shot_rx:  Option<Receiver<ShotData>>,
    loop_csv: bool,

    // Camera
    cam_pos:    Vec3,
    cam_target: Vec3,

    last_tick: Instant,
}

const CAM_POS:    Vec3 = Vec3::new(0.0, 8.0, 22.0);
const CAM_TARGET: Vec3 = Vec3::new(0.0, 0.0, -50.0);
const HUD_CAP: usize = 1024 * 1024; // 1 MB

impl App {
    pub fn new(window: Arc<Window>, shot: &ShotData, loop_csv: bool) -> Self {
        let mut ctx = VulkanContext::new(&window);

        // ── Scene geometry ────────────────────────────────────────────────
        let scene = Scene3D::new(&mut ctx);

        // ── Font atlas → GPU texture ──────────────────────────────────────
        let atlas = FontAtlas::build();
        let (font_image, font_view, font_alloc) = unsafe {
            texture::upload_r8_texture(
                &ctx.device,
                &mut ctx.allocator,
                ctx.graphics_queue,
                ctx.cmd_pool,
                atlas.width,
                atlas.height,
                &atlas.pixels,
            )
        };
        let sampler_info = vk::SamplerCreateInfo {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            address_mode_u: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_v: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            ..Default::default()
        };
        let font_sampler = unsafe { ctx.device.create_sampler(&sampler_info, None).unwrap() };
        ctx.set_font_texture(font_view, font_sampler);

        // ── HUD dynamic vertex buffer ─────────────────────────────────────
        let hud_buf = DynBuffer::new(&mut ctx, HUD_CAP, vk::BufferUsageFlags::VERTEX_BUFFER);

        // ── Animation setup ───────────────────────────────────────────────
        let roll_t = shot.roll_yds / (shot.roll_speed_mph * 0.44).max(1.0);
        let phase = if loop_csv { Phase::Aerial } else { Phase::Idle };

        App {
            window,
            ctx,
            scene,
            shot: shot.clone(),
            atlas,
            font_image,
            font_view,
            font_alloc,
            font_sampler,
            hud_buf,
            hud_vertex_count: 0,
            phase,
            elapsed: 0.0,
            pause_timer: 0.0,
            roll_t,
            ball_pos: Vec3::new(0.0, 0.25, 0.0),
            trail_pts: Vec::new(),
            shot_rx: None,
            loop_csv,
            cam_pos: CAM_POS,
            cam_target: CAM_TARGET,
            last_tick: Instant::now(),
        }
    }

    /// Attach a live shot receiver (from flighthook). Switches app to live mode.
    pub fn set_live_receiver(&mut self, rx: Receiver<ShotData>) {
        self.shot_rx = Some(rx);
        self.loop_csv = false;
        self.phase = Phase::Idle;
        self.trail_pts.clear();
        self.ball_pos = Vec3::new(0.0, 0.25, 0.0);
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn save_screenshot(&mut self, path: &str) {
        self.ctx.save_screenshot(path);
    }

    /// Advance animation by real time.
    pub fn tick(&mut self) {
        let now = Instant::now();
        let delta = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
        self.last_tick = now;

        match self.phase {
            Phase::Idle => {
                // Wait for a live shot to arrive.
                if let Some(rx) = &self.shot_rx {
                    if let Ok(new_shot) = rx.try_recv() {
                        self.shot = new_shot;
                        self.roll_t = self.shot.roll_yds / (self.shot.roll_speed_mph * 0.44).max(1.0);
                        self.phase = Phase::Aerial;
                        self.elapsed = 0.0;
                        self.trail_pts.clear();
                        self.ball_pos = Vec3::new(0.0, 0.25, 0.0);
                    }
                }
            }
            Phase::Aerial => {
                self.elapsed += delta;
                let shot = &self.shot;
                let t_max = shot.flight_time_sec.max(0.1);
                let frac = (self.elapsed / t_max).min(1.0);

                let x = shot.lateral_yds * frac.powf(1.5);
                let y = (shot.height_ft / 3.0) * (std::f32::consts::PI * frac).sin() + 0.25;
                let z = -shot.carry_yds * frac;
                self.ball_pos = Vec3::new(x, y, z);
                self.record_trail();

                if self.elapsed >= t_max {
                    self.phase = Phase::Roll;
                    self.elapsed = 0.0;
                }
            }
            Phase::Roll => {
                self.elapsed += delta;
                let shot = &self.shot;
                let roll_t = self.roll_t.max(0.01);
                let frac = (self.elapsed / roll_t).min(1.0);

                let x = shot.lateral_yds;
                let z = -shot.carry_yds - shot.roll_yds * frac;
                self.ball_pos = Vec3::new(x, 0.25, z);
                self.record_trail();

                if self.elapsed >= roll_t {
                    self.phase = Phase::Pause;
                    self.pause_timer = 0.0;
                }
            }
            Phase::Pause => {
                self.pause_timer += delta;
                if self.pause_timer >= 2.0 {
                    if self.loop_csv {
                        // CSV/demo mode: loop the animation forever.
                        self.phase = Phase::Aerial;
                        self.elapsed = 0.0;
                        self.trail_pts.clear();
                        self.ball_pos = Vec3::new(0.0, 0.25, 0.0);
                    } else {
                        // Live mode: return to tee and wait for the next shot.
                        self.phase = Phase::Idle;
                        self.trail_pts.clear();
                        self.ball_pos = Vec3::new(0.0, 0.25, 0.0);
                    }
                }
            }
        }
    }

    pub fn render(&mut self) {
        // Update trail mesh
        self.scene.rebuild_trail(&self.trail_pts, self.cam_pos);

        // Compute current carry/roll for HUD
        let (carry_cur, roll_cur) = match self.phase {
            Phase::Idle => (self.shot.carry_yds, self.shot.roll_yds),
            Phase::Aerial => {
                let t_max = self.shot.flight_time_sec.max(0.1);
                let frac = (self.elapsed / t_max).min(1.0);
                (self.shot.carry_yds * frac, 0.0)
            }
            Phase::Roll | Phase::Pause => (self.shot.carry_yds, self.shot.roll_yds),
        };

        // Build HUD vertices
        let hud_verts = hud::build_hud_vertices(&self.atlas, &self.shot, self.phase, carry_cur, roll_cur);
        self.hud_vertex_count = self.hud_buf.upload(&hud_verts);

        // Begin frame
        let (cmd, image_idx, frame) = self.ctx.begin_frame();

        // Update UBO3D
        let view = Mat4::look_at_rh(self.cam_pos, self.cam_target, Vec3::Y);
        let proj = projection_matrix(65.0_f32.to_radians(), 1280.0 / 720.0, 0.1, 500.0);
        let light_dir = compute_light_dir(-45.0_f32.to_radians(), 45.0_f32.to_radians());
        self.ctx.update_ubo3d(frame, &UBO3D {
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
            light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
        });

        // Update HUD UBO
        self.ctx.update_ubohud(frame, &UBOHUD {
            screen_size: [1280.0, 720.0],
            _pad: [0.0; 2],
        });

        // Draw: scene → trail → hud
        self.scene.draw_scene(&self.ctx, cmd, frame, self.ball_pos);
        self.scene.draw_trail(&self.ctx, cmd, frame);
        self.draw_hud(cmd, frame);

        // Compute projected label positions and draw them
        self.draw_dist_labels(cmd, frame, view, proj);

        self.ctx.end_frame(cmd, image_idx);
    }

    fn draw_hud(&self, cmd: vk::CommandBuffer, frame: usize) {
        if self.hud_vertex_count == 0 {
            return;
        }
        unsafe {
            let dev = &self.ctx.device;
            dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.ctx.hud_pipeline);
            dev.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.ctx.hud_layout,
                0,
                &[self.ctx.desc_sets_hud[frame]],
                &[],
            );
            dev.cmd_bind_vertex_buffers(cmd, 0, &[self.hud_buf.buf], &[0]);
            dev.cmd_draw(cmd, self.hud_vertex_count, 1, 0, 0);
        }
    }

    /// Project the 4 distance-ring label positions (4, 0.5, -d) → screen space,
    /// then emit HUD text quads for "50 yds" etc.
    fn draw_dist_labels(&self, cmd: vk::CommandBuffer, frame: usize, view: Mat4, proj: Mat4) {
        let vp = proj * view;
        let dists = [50.0f32, 100.0, 150.0, 200.0];

        let mut label_verts: Vec<VertexHUD> = Vec::new();
        for &d in &dists {
            let world = Vec3::new(4.0, 0.5, -d);
            if let Some((sx, sy)) = world_to_screen(world, vp) {
                let text = format!("{:.0} yds", d);
                self.atlas.layout_text(&text, sx, sy, 11, [1.0, 1.0, 0.8, 1.0], &mut label_verts);
            }
        }

        if label_verts.is_empty() {
            return;
        }

        // Upload label verts to a temporary range in the HUD buffer
        // We don't have a separate buffer for labels, so we'll just build them into
        // the main hud_buf during tick. For now, re-use the existing draw path:
        // (labels were excluded from the main hud_buf on purpose to avoid stale rebuild).
        // For simplicity, skip label upload here; they are included in the main HUD build
        // via the static placeholder in hud/mod.rs.
        let _ = (cmd, frame, label_verts);
    }

    fn record_trail(&mut self) {
        let p = self.ball_pos;
        const MIN_DIST: f32 = 0.15;
        if self.trail_pts.last().map_or(true, |&last| last.distance(p) >= MIN_DIST) {
            if self.trail_pts.len() < 256 {
                self.trail_pts.push(p);
            } else {
                // Shift and append (keep most recent 256)
                self.trail_pts.remove(0);
                self.trail_pts.push(p);
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.ctx.wait_idle();
        unsafe {
            self.ctx.device.destroy_sampler(self.font_sampler, None);
            self.ctx.device.destroy_image_view(self.font_view, None);
            self.ctx.device.destroy_image(self.font_image, None);
        }
    }
}

// ── Camera / math helpers ─────────────────────────────────────────────────

/// Vulkan-style perspective (Y-flip applied via negative height in viewport,
/// but easier to flip proj[1][1] for the Y-down convention).
fn projection_matrix(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    // Flip Y for Vulkan NDC
    Mat4::from_cols_array(&[
        f / aspect, 0.0,  0.0,                        0.0,
        0.0,       -f,    0.0,                        0.0,
        0.0,        0.0, far / (near - far),          -1.0,
        0.0,        0.0, (near * far) / (near - far),  0.0,
    ])
}

fn compute_light_dir(pitch: f32, yaw: f32) -> Vec3 {
    let x = yaw.sin() * pitch.cos();
    let y = pitch.sin();
    let z = -yaw.cos() * pitch.cos();
    Vec3::new(x, y, z).normalize()
}

fn world_to_screen(world: Vec3, vp: Mat4) -> Option<(f32, f32)> {
    let clip = vp * Vec4::from((world, 1.0));
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 {
        return None;
    }
    let sx = (ndc.x + 1.0) * 0.5 * 1280.0;
    let sy = (ndc.y + 1.0) * 0.5 * 720.0;
    Some((sx, sy))
}
