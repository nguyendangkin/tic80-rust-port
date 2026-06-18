//! Deprecated textured triangle rasterizer (`ttri`).
//!
//! Port of TIC-80's `src/core/draw_dep.c`.
//!
//! Uses scanline-based UV-mapped triangle rendering with a
//! fixed-point (16.16) perspective-correct rasterizer.

use crate::draw::{get_palette, set_pixel, ClipRect};
use crate::tilesheet;
// unused import removed

const TRANSPARENT_COLOR: u8 = 255;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TIC80_WIDTH: i32 = 240;
const TIC80_HEIGHT: i32 = 136;
const TIC_SPRITESIZE: i32 = 8;
const TIC_SPRITESHEET_SIZE: i32 = 128;
const TIC_SPRITE_BANKS: i32 = 4;
const TIC_MAP_WIDTH: i32 = 30;
const TIC_MAP_HEIGHT: i32 = 30;

// ---------------------------------------------------------------------------
// Global sides buffer (thread-local, like C static)
// ---------------------------------------------------------------------------


struct TexSideBuffer {
    left: [i16; 136],
    right: [i16; 136],
    uleft: [i32; 136],
    vleft: [i32; 136],
}

impl TexSideBuffer {
    fn new() -> Self {
        TexSideBuffer {
            left: [0i16; 136],
            right: [0i16; 136],
            uleft: [0i32; 136],
            vleft: [0i32; 136],
        }
    }
}

// SAFETY: single-threaded (matches C semantics)
unsafe impl Sync for TexSideBuffer {}

static mut SIDES: TexSideBuffer = TexSideBuffer {
    left: [0i16; 136],
    right: [0i16; 136],
    uleft: [0i32; 136],
    vleft: [0i32; 136],
};

unsafe fn sides() -> &'static mut TexSideBuffer {
    &mut SIDES
}

// replaced by direct unsafe fn sides()

// ---------------------------------------------------------------------------
// Texture vertex
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct TexVert {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
}

// ---------------------------------------------------------------------------
// Side buffer helpers
// ---------------------------------------------------------------------------

fn set_side_tex_pixel(x: i32, y: i32, u: f32, v: f32) {
    if y >= 0 && y < TIC80_HEIGHT {
        unsafe {
            let buf = sides();
            let yy = y as usize;
            if x < buf.left[yy] as i32 {
                buf.left[yy] = x as i16;
                buf.uleft[yy] = (u * 65536.0) as i32;
                buf.vleft[yy] = (v * 65536.0) as i32;
            }
            if x > buf.right[yy] as i32 {
                buf.right[yy] = x as i16;
            }
        }
    }
}

fn tex_line(v0: &TexVert, v1: &TexVert) {
    let (top, bot) = if v1.y < v0.y { (v1, v0) } else { (v0, v1) };

    let dy = bot.y - top.y;
    let mut step_x = bot.x - top.x;
    let mut step_u = bot.u - top.u;
    let mut step_v = bot.v - top.v;

    if dy as i32 != 0 {
        step_x /= dy;
        step_u /= dy;
        step_v /= dy;
    }

    let mut x = top.x;
    let mut y = top.y;
    let mut u = top.u;
    let mut v = top.v;

    if y < 0.0 {
        let clip = -y;
        x += step_x * clip;
        u += step_u * clip;
        v += step_v * clip;
        y = 0.0;
    }

    let mut bot_y = bot.y as i32;
    if bot_y > TIC80_HEIGHT {
        bot_y = TIC80_HEIGHT;
    }

    while y < bot_y as f32 {
        set_side_tex_pixel(x as i32, y as i32, u, v);
        x += step_x;
        u += step_u;
        v += step_v;
        y += 1.0;
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Render a textured triangle.
///
/// This is the deprecated `ttri()` TIC-80 API. Uses scanline rasterization
/// with UV mapping. Supports both direct tile and map-source modes.
#[allow(clippy::too_many_arguments)]
pub fn tic_api_textri(
    ram: &mut [u8],
    clip: &ClipRect,
    x1: f32, y1: f32,
    x2: f32, y2: f32,
    x3: f32, y3: f32,
    u1: f32, v1: f32,
    u2: f32, v2: f32,
    u3: f32, v3: f32,
    use_map: bool,
    colors: &[u8],
    _count: u8,
    blit_segment: u8,
    map_data: &[u8],
    ram_tiles: *mut u8,
    ram_font: *mut u8,
) {
    let mapping = get_palette(&[0u8; 8], colors); // simplified palette

    // Get tilesheet
    let src = match blit_segment {
        0 | 1 => ram_font,
        _ => ram_tiles,
    };
    let sheet = tilesheet::get(blit_segment, src);

    let v0 = TexVert { x: x1, y: y1, u: u1, v: v1 };
    let v1 = TexVert { x: x2, y: y2, u: u2, v: v2 };
    let v2 = TexVert { x: x3, y: y3, u: u3, v: v3 };

    // Compute UV slope across the surface
    let denom = (v0.x - v2.x) * (v1.y - v2.y) - (v1.x - v2.x) * (v0.y - v2.y);
    if denom == 0.0 { return; }
    let id = 1.0 / denom;

    let dudx = ((v0.u - v2.u) * (v1.y - v2.y) - (v1.u - v2.u) * (v0.y - v2.y)) * id;
    let dvdx = ((v0.v - v2.v) * (v1.y - v2.y) - (v1.v - v2.v) * (v0.y - v2.y)) * id;

    let dudxs = (dudx * 65536.0) as i32;
    let dvdxs = (dvdx * 65536.0) as i32;

    // Init side buffer
    unsafe {
        let buf = sides();
        for i in 0..TIC80_HEIGHT as usize {
            buf.left[i] = TIC80_WIDTH as i16;
            buf.right[i] = -1;
            buf.uleft[i] = 0;
            buf.vleft[i] = 0;
        }
    }

    // Rasterize triangle edges
    tex_line(&v0, &v1);
    tex_line(&v1, &v2);
    tex_line(&v2, &v0);

    // Fill scanlines
    for y in 0..TIC80_HEIGHT {
        unsafe {
            let buf = sides();
            let yy = y as usize;
            let width = buf.right[yy] as i32 - buf.left[yy] as i32;
            if width <= 0 { continue; }
            if y < clip.t || y > clip.b { continue; }

            let mut u = buf.uleft[yy];
            let mut v = buf.vleft[yy];
            let mut left = buf.left[yy] as i32;
            let mut right = buf.right[yy] as i32;

            if right > clip.r { right = clip.r; }
            if left < clip.l {
                let dist = clip.l - left;
                u += dudxs * dist;
                v += dvdxs * dist;
                left = clip.l;
            }

            if use_map {
                let map_w = TIC_MAP_WIDTH * TIC_SPRITESIZE;
                let map_h = TIC_MAP_HEIGHT * TIC_SPRITESIZE;
                for x in left..right {
                    let iu = ((u >> 16) % map_w + map_w) % map_w;
                    let iv = ((v >> 16) % map_h + map_h) % map_h;

                    let tile_index = map_data[((iv >> 3) * TIC_MAP_WIDTH + (iu >> 3)) as usize];
                    let tile = tilesheet::get_tile(&sheet, tile_index as u32, true);
                    let pix = tilesheet::get_tile_pix(&tile, iu & 7, iv & 7);
                    let color = mapping[pix as usize];
                    if color != TRANSPARENT_COLOR {
                        set_pixel(ram, clip, x, y, color);
                    }
                    u += dudxs;
                    v += dvdxs;
                }
            } else {
                let sheet_w = TIC_SPRITESHEET_SIZE;
                let sheet_h = TIC_SPRITESHEET_SIZE * TIC_SPRITE_BANKS;
                for x in left..right {
                    let iu = (u >> 16) & (sheet_w - 1);
                    let iv = (v >> 16) & (sheet_h - 1);

                    let pix = tilesheet::get_pix(&sheet, iu, iv);
                    let color = mapping[pix as usize];
                    if color != TRANSPARENT_COLOR {
                        set_pixel(ram, clip, x, y, color);
                    }
                    u += dudxs;
                    v += dvdxs;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    fn test_ram() -> Vec<u8> {
        vec![0u8; 240 * 136 / 2] // 4bpp screen buffer
    }
    fn test_clip() -> ClipRect {
        ClipRect { l: 0, t: 0, r: 240, b: 136 }
    }

    #[test]
    fn ttri_no_crash() {
        let mut ram = test_ram();
        let clip = test_clip();
        let map = [0u8; 30 * 30];
        let mut tiles = [0u8; 4096];
        let mut font = [0u8; 4096];

        // Just check it doesn't crash with a simple triangle
        tic_api_textri(
            &mut ram, &clip,
            10.0, 10.0, 50.0, 10.0, 30.0, 50.0,
            0.0, 0.0, 1.0, 0.0, 0.5, 1.0,
            false, &[], 0, 2,
            &map, tiles.as_mut_ptr(), font.as_mut_ptr(),
        );
        // No assertions — just checking it runs without panic
    }

    #[test]
    fn ttri_zero_denom() {
        let mut ram = test_ram();
        let clip = test_clip();
        let map = [0u8; 30 * 30];
        let mut tiles = [0u8; 4096];
        let mut font = [0u8; 4096];

        // Degenerate triangle (colinear) — denom = 0, should return early
        tic_api_textri(
            &mut ram, &clip,
            0.0, 0.0, 10.0, 0.0, 20.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.5, 1.0,
            false, &[], 0, 2,
            &map, tiles.as_mut_ptr(), font.as_mut_ptr(),
        );
    }

    #[test]
    fn ttri_map_mode() {
        let mut ram = test_ram();
        let clip = test_clip();
        let mut map = [0u8; 30 * 30];
        map[0] = 1; // tile 1 at (0,0)
        let mut tiles = [0u8; 4096];
        let mut font = [0u8; 4096];

        tic_api_textri(
            &mut ram, &clip,
            0.0, 0.0, 50.0, 0.0, 25.0, 50.0,
            0.0, 0.0, 8.0, 0.0, 4.0, 8.0,
            true, &[], 0, 2,
            &map, tiles.as_mut_ptr(), font.as_mut_ptr(),
        );
    }
}
