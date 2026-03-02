use std::collections::HashMap;

const ATLAS_SIZE: u32 = 512;
const FONT_BYTES: &[u8] = include_bytes!("../../assets/font.ttf");

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub advance_x: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub width: u32,
    pub height: u32,
}

pub struct FontAtlas {
    /// R8_UNORM pixel data, ATLAS_SIZE × ATLAS_SIZE
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// (char, px_size) → GlyphInfo
    pub glyphs: HashMap<(char, u32), GlyphInfo>,
}

impl FontAtlas {
    /// Rasterise printable ASCII at all required sizes and pack into one atlas.
    pub fn build() -> Self {
        let font = fontdue::Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default())
            .expect("Failed to load embedded font");

        let sizes: &[u32] = &[11, 12, 13, 14, 17];
        let chars: Vec<char> = (32u8..=126u8).map(|b| b as char).collect();

        let atlas_w = ATLAS_SIZE;
        let atlas_h = ATLAS_SIZE;
        let mut pixels = vec![0u8; (atlas_w * atlas_h) as usize];
        let mut glyphs = HashMap::new();

        let mut cur_x: u32 = 0;
        let mut cur_y: u32 = 0;
        let mut row_h: u32 = 0;
        let padding: u32 = 1;

        for &sz in sizes {
            for &ch in &chars {
                let (metrics, bitmap) =
                    font.rasterize(ch, sz as f32);

                let gw = metrics.width as u32;
                let gh = metrics.height as u32;

                if gw == 0 || gh == 0 {
                    // Whitespace / invisible glyph – still record advance
                    glyphs.insert(
                        (ch, sz),
                        GlyphInfo {
                            uv_min: [0.0; 2],
                            uv_max: [0.0; 2],
                            advance_x: metrics.advance_width,
                            bearing_x: metrics.bounds.xmin,
                            bearing_y: metrics.bounds.ymin,
                            width: 0,
                            height: 0,
                        },
                    );
                    continue;
                }

                if cur_x + gw + padding > atlas_w {
                    cur_x = 0;
                    cur_y += row_h + padding;
                    row_h = 0;
                }

                if cur_y + gh > atlas_h {
                    // Out of atlas space – skip (won't happen for our sizes)
                    continue;
                }

                // Blit bitmap into atlas
                for row in 0..gh {
                    let src_off = (row * gw) as usize;
                    let dst_off = ((cur_y + row) * atlas_w + cur_x) as usize;
                    pixels[dst_off..dst_off + gw as usize]
                        .copy_from_slice(&bitmap[src_off..src_off + gw as usize]);
                }

                let uv_min = [
                    cur_x as f32 / atlas_w as f32,
                    cur_y as f32 / atlas_h as f32,
                ];
                let uv_max = [
                    (cur_x + gw) as f32 / atlas_w as f32,
                    (cur_y + gh) as f32 / atlas_h as f32,
                ];

                glyphs.insert(
                    (ch, sz),
                    GlyphInfo {
                        uv_min,
                        uv_max,
                        advance_x: metrics.advance_width,
                        bearing_x: metrics.bounds.xmin,
                        bearing_y: metrics.bounds.ymin,
                        width: gw,
                        height: gh,
                    },
                );

                cur_x += gw + padding;
                if gh > row_h {
                    row_h = gh;
                }
            }
        }

        FontAtlas {
            pixels,
            width: atlas_w,
            height: atlas_h,
            glyphs,
        }
    }

    /// Measure the pixel width of a string at a given size.
    pub fn text_width(&self, text: &str, px: u32) -> f32 {
        text.chars()
            .filter_map(|c| self.glyphs.get(&(c, px)))
            .map(|g| g.advance_x)
            .sum()
    }

    /// Emit VertexHUD quads for `text` starting at (x, y) baseline.
    /// Returns (vertices, new_x).
    pub fn layout_text(
        &self,
        text: &str,
        mut x: f32,
        y: f32,
        px: u32,
        color: [f32; 4],
        out: &mut Vec<crate::hud::VertexHUD>,
    ) -> f32 {
        for ch in text.chars() {
            let Some(g) = self.glyphs.get(&(ch, px)) else {
                continue;
            };
            if g.width > 0 && g.height > 0 {
                let x0 = x + g.bearing_x;
                // fontdue bearing_y is the bottom of the glyph relative to baseline
                let y0 = y - g.bearing_y - g.height as f32;
                let x1 = x0 + g.width as f32;
                let y1 = y0 + g.height as f32;
                push_glyph_quad(out, x0, y0, x1, y1, g.uv_min, g.uv_max, color);
            }
            x += g.advance_x;
        }
        x
    }
}

/// Emit a textured quad (2 triangles, 6 vertices) for one glyph.
fn push_glyph_quad(
    out: &mut Vec<crate::hud::VertexHUD>,
    x0: f32, y0: f32, x1: f32, y1: f32,
    uv_min: [f32; 2], uv_max: [f32; 2],
    color: [f32; 4],
) {
    use crate::hud::VertexHUD;
    let tl = VertexHUD { pos: [x0, y0], uv: [uv_min[0], uv_min[1]], color, use_tex: 1.0 };
    let tr = VertexHUD { pos: [x1, y0], uv: [uv_max[0], uv_min[1]], color, use_tex: 1.0 };
    let bl = VertexHUD { pos: [x0, y1], uv: [uv_min[0], uv_max[1]], color, use_tex: 1.0 };
    let br = VertexHUD { pos: [x1, y1], uv: [uv_max[0], uv_max[1]], color, use_tex: 1.0 };
    out.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
}
