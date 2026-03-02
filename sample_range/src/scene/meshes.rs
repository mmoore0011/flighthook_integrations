use std::f32::consts::PI;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex3D {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
}

pub struct MeshData {
    pub vertices: Vec<Vertex3D>,
    pub indices: Vec<u32>,
}

/// Flat unit quad in the XZ plane (normal = +Y), centred at origin.
/// Scale/translate via push constants.
pub fn quad_xz(width: f32, depth: f32) -> MeshData {
    let hw = width * 0.5;
    let hd = depth * 0.5;
    let n = [0.0_f32, 1.0, 0.0];
    let vertices = vec![
        Vertex3D { pos: [-hw, 0.0, -hd], normal: n },
        Vertex3D { pos: [ hw, 0.0, -hd], normal: n },
        Vertex3D { pos: [ hw, 0.0,  hd], normal: n },
        Vertex3D { pos: [-hw, 0.0,  hd], normal: n },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    MeshData { vertices, indices }
}

/// UV sphere: `rings` latitude bands × `sectors` longitude slices.
pub fn sphere(radius: f32, rings: u32, sectors: u32) -> MeshData {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for r in 0..=rings {
        let phi = PI * r as f32 / rings as f32; // 0..π
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();
        for s in 0..=sectors {
            let theta = 2.0 * PI * s as f32 / sectors as f32;
            let x = sin_phi * theta.cos();
            let y = cos_phi;
            let z = sin_phi * theta.sin();
            vertices.push(Vertex3D {
                pos: [x * radius, y * radius, z * radius],
                normal: [x, y, z],
            });
        }
    }

    for r in 0..rings {
        for s in 0..sectors {
            let a = r * (sectors + 1) + s;
            let b = a + sectors + 1;
            indices.push(a);
            indices.push(b);
            indices.push(a + 1);
            indices.push(b);
            indices.push(b + 1);
            indices.push(a + 1);
        }
    }

    MeshData { vertices, indices }
}

/// Flat cylinder (top+bottom caps, side quads) centred at origin, axis = Y.
pub fn cylinder(radius: f32, height: f32, segments: u32) -> MeshData {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let half_h = height * 0.5;

    // Side ring: top row then bottom row, interleaved
    for i in 0..=segments {
        let theta = 2.0 * PI * i as f32 / segments as f32;
        let x = theta.cos();
        let z = theta.sin();
        let n = [x, 0.0, z];
        vertices.push(Vertex3D { pos: [x * radius, -half_h, z * radius], normal: n });
        vertices.push(Vertex3D { pos: [x * radius,  half_h, z * radius], normal: n });
    }
    let side_verts = vertices.len() as u32;

    for i in 0..segments {
        let b = i * 2;
        let t = b + 1;
        let nb = b + 2;
        let nt = b + 3;
        indices.push(b); indices.push(nb); indices.push(t);
        indices.push(t); indices.push(nb); indices.push(nt);
    }

    // Top cap (y = +half_h, normal = +Y)
    let top_center = vertices.len() as u32;
    vertices.push(Vertex3D { pos: [0.0, half_h, 0.0], normal: [0.0, 1.0, 0.0] });
    let top_start = vertices.len() as u32;
    for i in 0..segments {
        let theta = 2.0 * PI * i as f32 / segments as f32;
        let x = theta.cos() * radius;
        let z = theta.sin() * radius;
        vertices.push(Vertex3D { pos: [x, half_h, z], normal: [0.0, 1.0, 0.0] });
    }
    for i in 0..segments {
        indices.push(top_center);
        indices.push(top_start + i);
        indices.push(top_start + (i + 1) % segments);
    }

    // Bottom cap (y = -half_h, normal = -Y)
    let bot_center = vertices.len() as u32;
    vertices.push(Vertex3D { pos: [0.0, -half_h, 0.0], normal: [0.0, -1.0, 0.0] });
    let bot_start = vertices.len() as u32;
    for i in 0..segments {
        let theta = 2.0 * PI * i as f32 / segments as f32;
        let x = theta.cos() * radius;
        let z = theta.sin() * radius;
        vertices.push(Vertex3D { pos: [x, -half_h, z], normal: [0.0, -1.0, 0.0] });
    }
    for i in 0..segments {
        indices.push(bot_center);
        indices.push(bot_start + (i + 1) % segments);
        indices.push(bot_start + i);
    }

    let _ = side_verts; // suppress warning
    MeshData { vertices, indices }
}
