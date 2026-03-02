pub mod font_atlas;
pub use font_atlas::FontAtlas;

use crate::shot_data::ShotData;

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct VertexHUD {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub use_tex: f32,
}

const GOLD:  [f32; 4] = [1.0, 0.8, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BG:    [f32; 4] = [0.0, 0.0, 0.0, 0.65];

/// Build the complete HUD vertex list for one frame.
/// `carry_cur` / `roll_cur` are animation-driven distances in yards.
pub fn build_hud_vertices(
    atlas: &FontAtlas,
    shot: &ShotData,
    phase: crate::app::Phase,
    carry_cur: f32,
    roll_cur: f32,
) -> Vec<VertexHUD> {
    let mut v: Vec<VertexHUD> = Vec::with_capacity(8192);

    // ── Top bar ──────────────────────────────────────────────────────────
    push_rect(&mut v, 0.0, 0.0, 1280.0, 40.0, BG);
    let y_top = 26.0; // baseline
    atlas.layout_text(&format!("Club: {}", shot.club), 10.0, y_top, 17, GOLD, &mut v);
    let mid_text = format!("Shot Type: {}", shot.shot_type);
    let mw = atlas.text_width(&mid_text, 17);
    atlas.layout_text(&mid_text, 640.0 - mw * 0.5, y_top, 17, GOLD, &mut v);
    let player_text = format!("Player: {}", shot.player);
    let pw = atlas.text_width(&player_text, 14);
    atlas.layout_text(&player_text, 960.0 - pw * 0.5, y_top, 14, WHITE, &mut v);
    let tw = atlas.text_width(&shot.timestamp, 12);
    atlas.layout_text(&shot.timestamp, 1270.0 - tw, y_top, 12, WHITE, &mut v);

    // ── Left panel ───────────────────────────────────────────────────────
    push_rect(&mut v, 0.0, 40.0, 185.0, 510.0, BG);
    atlas.layout_text("DISTANCES", 5.0, 58.0, 13, GOLD, &mut v);

    // Live-updating carry/roll/total
    let (carry_str, roll_ft_str, roll_yds_str, total_ft_str, total_yds_str) = match phase {
        crate::app::Phase::Aerial => {
            (
                format!("{:.1} yds", carry_cur),
                "--".into(),
                "--".into(),
                "--".into(),
                "--".into(),
            )
        }
        crate::app::Phase::Idle | crate::app::Phase::Roll | crate::app::Phase::Pause => {
            let total = carry_cur + roll_cur;
            (
                format!("{:.1} yds", carry_cur),
                format!("{:.1} ft", roll_cur * 3.0),
                format!("{:.1} yds", roll_cur),
                format!("{:.1} ft", total * 3.0),
                format!("{:.1} yds", total),
            )
        }
    };

    let left_stats: &[(&str, &str)] = &[
        ("Carry",      &carry_str),
        ("Roll",       &roll_ft_str),
        ("Roll",       &roll_yds_str),
        ("Total",      &total_ft_str),
        ("Total",      &total_yds_str),
        ("Lateral",    &fmt_dir(shot.lateral_yds, "yds")),
        ("Curve Dist", &fmt_dir(shot.curve_dist_yds, "yds")),
        ("Height",     &format!("{:.1} ft", shot.height_ft)),
        ("Skid Dist",  &format!("{:.1} ft", shot.skid_distance_ft)),
        ("Flight Time",&format!("{:.1} s", shot.flight_time_sec)),
    ];
    let mut sy = 76.0f32;
    for (name, val) in left_stats {
        atlas.layout_text(name, 5.0, sy, 11, GOLD, &mut v);
        atlas.layout_text(val,  90.0, sy, 11, WHITE, &mut v);
        sy += 18.0;
    }

    // ── Right panel ──────────────────────────────────────────────────────
    push_rect(&mut v, 1095.0, 40.0, 185.0, 510.0, BG);
    atlas.layout_text("ANGLES & SPIN", 1100.0, 58.0, 13, GOLD, &mut v);

    let right_stats: &[(&str, String)] = &[
        ("Spin",       format!("{:.0} rpm", shot.spin_rpm)),
        ("Spin Axis",  fmt_dir_deg(shot.spin_axis_deg)),
        ("Launch V",   format!("{:.1}°", shot.launch_v_deg)),
        ("Launch H",   fmt_dir_deg(shot.launch_h_deg)),
        ("V-Plane",    format!("{:.1}°", shot.v_plane_deg)),
        ("H-Plane",    fmt_dir_deg(shot.h_plane_deg)),
        ("Ball Dir",   fmt_dir_deg(shot.ball_direction_deg)),
        ("Descent V",  format!("{:.1}°", shot.descent_v_deg)),
        ("AOA",        format!("{:.1}°", shot.aoa_deg)),
        ("FTT",        fmt_dir_deg(shot.ftt_deg)),
    ];
    let mut sy = 76.0f32;
    for (name, val) in right_stats {
        atlas.layout_text(name, 1100.0, sy, 11, GOLD, &mut v);
        atlas.layout_text(val, 1185.0, sy, 11, WHITE, &mut v);
        sy += 18.0;
    }

    // ── Bottom bar ───────────────────────────────────────────────────────
    push_rect(&mut v, 0.0, 550.0, 1280.0, 170.0, BG);
    atlas.layout_text("SPEED & CLUB",        6.0, 564.0, 11, GOLD, &mut v);
    atlas.layout_text("IMPACT & EFFICIENCY", 6.0, 650.0, 11, GOLD, &mut v);

    let row1: &[(&str, String)] = &[
        ("Club Speed", format!("{:.1} mph", shot.club_speed_mph)),
        ("Ball Speed", format!("{:.1} mph", shot.ball_speed_mph)),
        ("Lnch Speed", format!("{:.1} mph", shot.launch_speed_mph)),
        ("Smash",      format!("{:.2}", shot.smash)),
        ("Roll Speed", format!("{:.1} mph", shot.roll_speed_mph)),
        ("Club Path",  fmt_dir_deg(shot.club_path_deg)),
        ("Spin Loft",  format!("{:.1}°", shot.spin_loft_deg)),
    ];
    let row2: &[(&str, String)] = &[
        ("Dyn. Loft",  format!("{:.1}°", shot.dynamic_loft_deg)),
        ("FTP",        fmt_dir_deg(shot.ftp_deg)),
        ("Low Point",  format!("{:.1} in", shot.low_point_in)),
        ("Lat Impact", format!("{:.1} in", shot.lateral_impact_in)),
        ("Vert Impact",format!("{:.1} in", shot.vertical_impact_in)),
        ("Roll ft/s/ft",format!("{:.2}", shot.roll_ft_s_ft)),
        ("Skid ft/s/ft",format!("{:.2}", shot.skid_ft_s_ft)),
    ];
    emit_stat_row(atlas, &mut v, row1, 570.0);
    emit_stat_row(atlas, &mut v, row2, 656.0);

    v
}

fn emit_stat_row(
    atlas: &FontAtlas,
    v: &mut Vec<VertexHUD>,
    stats: &[(&str, String)],
    y_label: f32,
) {
    let col_w = 1270.0 / 7.0;
    for (i, (name, val)) in stats.iter().enumerate() {
        let x = 5.0 + i as f32 * col_w;
        atlas.layout_text(name, x, y_label,      11, GOLD,  v);
        atlas.layout_text(val,  x, y_label + 16.0, 14, WHITE, v);
    }
}

#[allow(dead_code)]
/// Build HUD vertices, then append projected 3D label positions from app.
pub fn build_hud_vertices_with_labels(
    atlas: &FontAtlas,
    shot: &ShotData,
    phase: crate::app::Phase,
    carry_cur: f32,
    roll_cur: f32,
    label_positions: &[(f32, f32, &str)], // (screen_x, screen_y, text)
) -> Vec<VertexHUD> {
    let mut v = build_hud_vertices(atlas, shot, phase, carry_cur, roll_cur);
    for &(sx, sy, text) in label_positions {
        atlas.layout_text(text, sx, sy, 11, [1.0, 1.0, 0.8, 1.0], &mut v);
    }
    v
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn push_rect(v: &mut Vec<VertexHUD>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let x1 = x + w;
    let y1 = y + h;
    let uv = [0.0, 0.0];
    let tl = VertexHUD { pos: [x,  y ], uv, color, use_tex: 0.0 };
    let tr = VertexHUD { pos: [x1, y ], uv, color, use_tex: 0.0 };
    let bl = VertexHUD { pos: [x,  y1], uv, color, use_tex: 0.0 };
    let br = VertexHUD { pos: [x1, y1], uv, color, use_tex: 0.0 };
    v.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
}

fn fmt_dir(val: f32, unit: &str) -> String {
    if val.abs() < 0.05 {
        return format!("0.0 {}", unit);
    }
    if val > 0.0 {
        format!("{:.1} R {}", val, unit)
    } else {
        format!("{:.1} L {}", val.abs(), unit)
    }
}

fn fmt_dir_deg(val: f32) -> String {
    if val.abs() < 0.05 {
        return "0.0°".into();
    }
    if val > 0.0 {
        format!("{:.1} R°", val)
    } else {
        format!("{:.1} L°", val.abs())
    }
}
