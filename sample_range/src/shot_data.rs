/// All data from one FlightScope shot.
#[derive(Debug, Clone, Default)]
pub struct ShotData {
    // Identity
    pub player: String,
    pub timestamp: String,
    pub club: String,
    pub shot_type: String,
    // Distance
    pub carry_yds: f32,
    pub roll_ft: f32,
    pub roll_yds: f32,
    pub total_ft: f32,
    pub total_yds: f32,
    pub lateral_yds: f32,
    pub curve_dist_yds: f32,
    pub height_ft: f32,
    pub skid_distance_ft: f32,
    pub flight_time_sec: f32,
    // Speed
    pub ball_speed_mph: f32,
    pub launch_speed_mph: f32,
    pub club_speed_mph: f32,
    pub roll_speed_mph: f32,
    pub smash: f32,
    // Spin
    pub spin_rpm: f32,
    pub spin_axis_deg: f32,
    pub spin_loft_deg: f32,
    // Angles
    pub launch_v_deg: f32,
    pub launch_h_deg: f32,
    pub v_plane_deg: f32,
    pub h_plane_deg: f32,
    pub ball_direction_deg: f32,
    pub descent_v_deg: f32,
    pub aoa_deg: f32,
    // Club
    pub club_path_deg: f32,
    pub dynamic_loft_deg: f32,
    pub ftt_deg: f32,
    pub ftp_deg: f32,
    pub low_point_in: f32,
    // Impact
    pub lateral_impact_in: f32,
    pub vertical_impact_in: f32,
    // Efficiency
    pub roll_ft_s_ft: f32,
    pub skid_ft_s_ft: f32,
}

impl ShotData {
    /// Hardcoded values from 4_one_hit_session_export.csv.
    pub fn example() -> Self {
        ShotData {
            player: "Mike".into(),
            timestamp: "2026-02-25; 11-12-15".into(),
            club: "Lob Wedge".into(),
            shot_type: "PushDraw".into(),
            carry_yds: 108.1,
            roll_ft: 21.0,
            roll_yds: 7.0,
            total_ft: 345.417,
            total_yds: 115.1,
            lateral_yds: 3.8,
            curve_dist_yds: -6.4,
            height_ft: 44.7,
            skid_distance_ft: 0.0,
            flight_time_sec: 4.3,
            ball_speed_mph: 90.3,
            launch_speed_mph: 90.3,
            club_speed_mph: 81.7,
            roll_speed_mph: 0.0,
            smash: 1.11,
            spin_rpm: 9509.0,
            spin_axis_deg: -8.8,
            spin_loft_deg: 40.4,
            launch_v_deg: 16.3,
            launch_h_deg: 5.4,
            v_plane_deg: 60.4,
            h_plane_deg: 17.3,
            ball_direction_deg: 5.4,
            descent_v_deg: 37.4,
            aoa_deg: -10.9,
            club_path_deg: 11.1,
            dynamic_loft_deg: 28.7,
            ftt_deg: 2.8,
            ftp_deg: -8.2,
            low_point_in: 7.9,
            lateral_impact_in: 0.0,
            vertical_impact_in: -0.80,
            roll_ft_s_ft: 0.0,
            skid_ft_s_ft: 0.0,
        }
    }

    pub fn from_csv(path: &str) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        parse_csv(&content)
    }

    /// Build a ShotData from a flighthook `shot` JSON object and the message timestamp.
    pub fn from_flighthook(shot: &serde_json::Value, timestamp: &str) -> Option<Self> {
        let ball = shot.get("ball")?;
        let club = shot.get("club").filter(|v| !v.is_null());
        let spin = shot.get("spin").filter(|v| !v.is_null());

        let carry_yds       = parse_dist_yds(ball["carry_distance"].as_str()?)?;
        let total_yds       = parse_dist_yds(ball["total_distance"].as_str()?)?;
        let height_ft       = parse_dist_ft(ball["max_height"].as_str()?)?;
        let ball_speed_mph  = parse_vel_mph(ball["launch_speed"].as_str()?)?;
        let launch_v_deg    = ball["launch_elevation"].as_f64()? as f32;
        let launch_h_deg    = ball["launch_azimuth"].as_f64()? as f32;

        let roll_yds    = (total_yds - carry_yds).max(0.0);
        let lateral_yds = carry_yds * launch_h_deg.to_radians().sin();

        let spin_rpm = spin
            .and_then(|s| s["total_spin"].as_f64().map(|v| v as f32))
            .unwrap_or_else(|| {
                let back = ball["backspin_rpm"].as_f64().unwrap_or(0.0) as f32;
                let side = ball["sidespin_rpm"].as_f64().unwrap_or(0.0) as f32;
                (back * back + side * side).sqrt()
            });
        let spin_axis_deg = spin
            .and_then(|s| s["spin_axis"].as_f64().map(|v| v as f32))
            .unwrap_or(0.0);

        let club_speed_mph = club
            .and_then(|c| c["club_speed"].as_str())
            .and_then(parse_vel_mph)
            .unwrap_or(0.0);
        let smash = club
            .and_then(|c| c["smash_factor"].as_f64().map(|v| v as f32))
            .unwrap_or(0.0);
        let aoa_deg = club
            .and_then(|c| c["attack_angle"].as_f64().map(|v| v as f32))
            .unwrap_or(0.0);
        let club_path_deg = club
            .and_then(|c| c["path"].as_f64().map(|v| v as f32))
            .unwrap_or(0.0);
        let dynamic_loft_deg = club
            .and_then(|c| c["dynamic_loft"].as_f64().map(|v| v as f32))
            .unwrap_or(0.0);

        Some(ShotData {
            player:           "Live".into(),
            timestamp:        timestamp.to_string(),
            club:             String::new(),
            shot_type:        String::new(),
            carry_yds,
            total_yds,
            roll_yds,
            height_ft,
            lateral_yds,
            ball_speed_mph,
            launch_speed_mph: ball_speed_mph,
            launch_v_deg,
            launch_h_deg,
            spin_rpm,
            spin_axis_deg,
            club_speed_mph,
            smash,
            aoa_deg,
            club_path_deg,
            dynamic_loft_deg,
            flight_time_sec:  carry_yds / 30.0,
            roll_speed_mph:   5.0,
            ..ShotData::default()
        })
    }
}

// ── RFC-4180 CSV parsing ──────────────────────────────────────────────────

fn parse_csv(content: &str) -> Option<ShotData> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() < 2 {
        return None;
    }
    let values = parse_row(lines[1]);
    if values.len() < 41 {
        return None;
    }
    let mut s = ShotData::default();
    s.player          = field(&values, 1);
    s.timestamp       = field(&values, 2);
    s.carry_yds       = flt(&values, 3);
    s.roll_ft         = ft_in(&values, 4);
    s.roll_yds        = flt(&values, 5);
    s.total_ft        = ft_in(&values, 6);
    s.total_yds       = flt(&values, 7);
    s.lateral_yds     = dir(&values, 8);
    s.curve_dist_yds  = dir(&values, 9);
    s.smash           = flt(&values, 10);
    s.roll_ft_s_ft    = flt(&values, 11);
    s.roll_speed_mph  = flt(&values, 13);
    s.flight_time_sec = flt(&values, 14);
    s.skid_distance_ft= flt(&values, 15);
    s.spin_rpm        = flt(&values, 16);
    s.skid_ft_s_ft    = flt(&values, 17);
    s.spin_axis_deg   = dir(&values, 18);
    s.club_path_deg   = dir(&values, 19);
    s.ball_speed_mph  = flt(&values, 20);
    s.launch_speed_mph= flt(&values, 21);
    s.v_plane_deg     = flt(&values, 22);
    s.h_plane_deg     = dir(&values, 23);
    s.launch_v_deg    = flt(&values, 24);
    s.launch_h_deg    = dir(&values, 25);
    s.ball_direction_deg = dir(&values, 26);
    s.height_ft       = flt(&values, 27);
    s.ftt_deg         = dir(&values, 28);
    s.dynamic_loft_deg= flt(&values, 29);
    s.spin_loft_deg   = flt(&values, 30);
    s.club_speed_mph  = flt(&values, 31);
    s.descent_v_deg   = flt(&values, 33);
    s.aoa_deg         = flt(&values, 34);
    s.low_point_in    = flt(&values, 35);
    s.ftp_deg         = dir(&values, 36);
    s.club            = field(&values, 37);
    s.lateral_impact_in  = flt(&values, 38);
    s.vertical_impact_in = flt(&values, 39);
    s.shot_type       = field(&values, 40);
    Some(s)
}

fn parse_row(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let n = bytes.len();
    let mut fields = Vec::new();
    let mut i = 0;
    while i < n {
        if bytes[i] == b'"' {
            i += 1;
            let mut buf = Vec::new();
            while i < n {
                if bytes[i] == b'"' {
                    if i + 1 < n && bytes[i + 1] == b'"' {
                        buf.push(b'"');
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    buf.push(bytes[i]);
                    i += 1;
                }
            }
            fields.push(String::from_utf8_lossy(&buf).into_owned());
        } else {
            let start = i;
            while i < n && bytes[i] != b',' {
                i += 1;
            }
            fields.push(line[start..i].to_string());
        }
        if i < n && bytes[i] == b',' {
            i += 1;
            if i == n {
                fields.push(String::new());
            }
        }
    }
    fields
}

fn field(v: &[String], idx: usize) -> String {
    v.get(idx).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn flt(v: &[String], idx: usize) -> f32 {
    field(v, idx).parse().unwrap_or(0.0)
}

fn ft_in(v: &[String], idx: usize) -> f32 {
    let s = field(v, idx);
    if s.is_empty() {
        return 0.0;
    }
    if let Some(pos) = s.find('\'') {
        let feet: f32 = s[..pos].parse().unwrap_or(0.0);
        let inch_str = s[pos + 1..].trim_end_matches('"').trim();
        let inches: f32 = inch_str.parse().unwrap_or(0.0);
        return feet + inches / 12.0;
    }
    s.parse().unwrap_or(0.0)
}

fn dir(v: &[String], idx: usize) -> f32 {
    let s = field(v, idx);
    if s.is_empty() {
        return 0.0;
    }
    if s.ends_with(" R") {
        return s[..s.len() - 2].parse().unwrap_or(0.0);
    }
    if s.ends_with(" L") {
        return -s[..s.len() - 2].parse::<f32>().unwrap_or(0.0);
    }
    s.parse().unwrap_or(0.0)
}

// ── flighthook unit parsing ───────────────────────────────────────────────

/// Parse a unit-tagged distance string → yards.
/// Supports: `"180.5m"`, `"108.1yds"/"yd"`, `"50ft"`, `"24in"`.
fn parse_dist_yds(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("yds").or_else(|| s.strip_suffix("yd")) {
        return n.trim().parse().ok();
    }
    if let Some(n) = s.strip_suffix("ft") {
        return n.trim().parse::<f32>().ok().map(|v| v / 3.0);
    }
    if let Some(n) = s.strip_suffix("in") {
        return n.trim().parse::<f32>().ok().map(|v| v / 36.0);
    }
    if let Some(n) = s.strip_suffix('m') {
        return n.trim().parse::<f32>().ok().map(|v| v * 1.094);
    }
    None
}

/// Parse a unit-tagged distance string → feet.
fn parse_dist_ft(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("ft") {
        return n.trim().parse().ok();
    }
    if let Some(n) = s.strip_suffix("in") {
        return n.trim().parse::<f32>().ok().map(|v| v / 12.0);
    }
    if let Some(n) = s.strip_suffix("yds").or_else(|| s.strip_suffix("yd")) {
        return n.trim().parse::<f32>().ok().map(|v| v * 3.0);
    }
    if let Some(n) = s.strip_suffix('m') {
        return n.trim().parse::<f32>().ok().map(|v| v * 3.281);
    }
    None
}

/// Parse a unit-tagged velocity string → mph.
/// Supports: `"67.2mps"`, `"90mph"`, `"120kph"`, `"132fps"`.
fn parse_vel_mph(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("mps") {
        return n.trim().parse::<f32>().ok().map(|v| v * 2.237);
    }
    if let Some(n) = s.strip_suffix("mph") {
        return n.trim().parse().ok();
    }
    if let Some(n) = s.strip_suffix("kph") {
        return n.trim().parse::<f32>().ok().map(|v| v * 0.621);
    }
    if let Some(n) = s.strip_suffix("fps") {
        return n.trim().parse::<f32>().ok().map(|v| v * 0.682);
    }
    None
}
