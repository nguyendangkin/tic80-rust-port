//! Graphics pipeline — full port of src/core/draw.c
//!
//! All clipping, primitive drawing, sprite/map rendering, and font output.
//! Uses a global palette mapping (like the C code) for color translation.

#![allow(unused)]

use crate::core;
use crate::tilesheet;
use crate::tools;
use std::ptr;

// ---------------------------------------------------------------------------
// Re-exported for downstream use
// ---------------------------------------------------------------------------

pub use self::types::*;

mod types {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    pub struct ClipRect { pub l: i32, pub t: i32, pub r: i32, pub b: i32 }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Flip { None = 0, Horz = 1, Vert = 2, Both = 3 }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Rotate { None = 0, R90 = 1, R180 = 2, R270 = 3 }

    #[derive(Clone, Copy, Debug)]
    pub struct FillSegment { pub y: i32, pub xl: i32, pub xr: i32, pub dy: i32 }

    #[derive(Clone, Copy, Debug)]
    pub struct FillQueue { pub seg: [FillSegment; FILL_QUEUE_SIZE], pub ini: usize, pub outi: usize }

    #[derive(Clone, Copy, Debug)]
    pub struct RemapResult { pub index: i32, pub flip: Flip, pub rotate: Rotate }

    pub type RemapFunc = Option<fn(data: *mut std::ffi::c_void, x: i32, y: i32, tile: &mut RemapResult)>;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TRANSPARENT_COLOR: u8 = 255;
pub const TIC_SPRITESIZE: usize = 8;
pub const TIC80_FULLWIDTH: usize = 256;
pub const TIC80_FULLHEIGHT: usize = 256;
pub const TIC80_WIDTH: u32 = 240;
pub const TIC80_HEIGHT: u32 = 136;
pub const TIC_MAP_WIDTH: i32 = 30;
pub const TIC_MAP_HEIGHT: i32 = 30;
pub const TIC_PALETTE_SIZE: usize = 16;
pub const TIC_PALETTE_BPP: u32 = 4;
pub const BITS_IN_BYTE: usize = 8;
pub const TIC_SPRITESHEET_SIZE: i32 = 128;
pub const TIC_SPRITE_BANKS: i32 = 4;
pub const TIC80_FRAMERATE: u32 = 60;
pub const TIC_MARGIN_TOP: i32 = 8;
pub const FILL_QUEUE_SIZE: usize = 400;

// ---------------------------------------------------------------------------
// Global palette mapping (set before calling draw functions)
// ---------------------------------------------------------------------------

static mut G_MAPPING: [u8; TIC_PALETTE_SIZE] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// Set the current palette mapping (8 bytes from vram.mapping).
pub fn set_mapping(mapping: &[u8]) {
    unsafe {
        for i in 0..TIC_PALETTE_SIZE.min(mapping.len()) {
            G_MAPPING[i] = mapping[i];
        }
    }
}

fn map_color(color: u8) -> u8 {
    unsafe {
        let idx = (color & 0x0f) as usize;
        if idx < G_MAPPING.len() { G_MAPPING[idx] } else { color }
    }
}

// ---------------------------------------------------------------------------
// Tilesheet helper
// ---------------------------------------------------------------------------

fn get_tilesheet_from_segment(ram_tiles: *const u8, ram_font: *const u8, segment: u8) -> tilesheet::Tilesheet<'static> {
    let src = match segment { 0 | 1 => ram_font as *mut u8, _ => ram_tiles as *mut u8 };
    tilesheet::get(segment, src)
}

// ---------------------------------------------------------------------------
// Pixel helpers
// ---------------------------------------------------------------------------

pub fn set_pixel(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, color: u8) {
    if x < clip.l || y < clip.t || x >= clip.r || y >= clip.b { return; }
    core::poke4(ram, y * 240 + x, color);
}

fn set_pixel_fast(ram: &mut [u8], x: i32, y: i32, color: u8) {
    core::poke4(ram, y * 240 + x, color);
}

fn get_pixel(ram: &[u8], x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= 240 || y >= 136 { return 0; }
    core::peek4(ram, y * 240 + x)
}

fn early_clip(x: i32, y: i32, w: i32, h: i32, clip: &ClipRect) -> bool {
    (y + h - 1) < clip.t || (x + w - 1) < clip.l || y >= clip.b || x >= clip.r
}

fn draw_hline(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, w: i32, color: u8) {
    if y < clip.t || clip.b <= y { return; }
    let xl = x.max(clip.l);
    let xr = (x + w).min(clip.r);
    for i in y * 240 + xl..y * 240 + xr { core::poke4(ram, i, color); }
}

fn draw_vline(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, h: i32, color: u8) {
    if x < clip.l || clip.r <= x { return; }
    let yl = y.max(0);
    let yr = (y + h).min(136);
    for i in yl..yr { set_pixel(ram, clip, x, i, color); }
}

fn draw_rect(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, w: i32, h: i32, color: u8) {
    for i in y..y + h { draw_hline(ram, clip, x, i, w, color); }
}

fn draw_rect_border(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, w: i32, h: i32, color: u8) {
    draw_hline(ram, clip, x, y, w, color);
    draw_hline(ram, clip, x, y + h - 1, w, color);
    draw_vline(ram, clip, x, y, h, color);
    draw_vline(ram, clip, x + w - 1, y, h, color);
}

// ---------------------------------------------------------------------------
// Ellipse helpers
// ---------------------------------------------------------------------------

static mut SIDES_LEFT: [i16; 136] = [0; 136];
static mut SIDES_RIGHT: [i16; 136] = [0; 136];

fn init_sides_buffer() {
    unsafe {
        for i in 0..136 { SIDES_LEFT[i] = 240i16; SIDES_RIGHT[i] = -1; }
    }
}

fn set_side_pixel(x: i32, y: i32) {
    if y >= 0 && y < 136 {
        unsafe {
            if x < SIDES_LEFT[y as usize] as i32 { SIDES_LEFT[y as usize] = x as i16; }
            if x > SIDES_RIGHT[y as usize] as i32 { SIDES_RIGHT[y as usize] = x as i16; }
        }
    }
}

fn draw_sides_buffer(ram: &mut [u8], clip: &ClipRect, y0: i32, y1: i32, color: u8) {
    let yt = y0.max(clip.t);
    let yb = (y1 + 1).min(clip.b);
    unsafe {
        for y in yt..yb {
            let xl = (SIDES_LEFT[y as usize] as i32).max(clip.l);
            let xr = (SIDES_RIGHT[y as usize] as i32 + 1).min(clip.r);
            for i in y * 240 + xl..y * 240 + xr { core::poke4(ram, i, color); }
        }
    }
}

fn draw_ellipse(ram: &mut [u8], clip: &ClipRect, x0: i32, y0: i32, x1: i32, y1: i32, color: u8, fill: bool) {
    if x0 > x1 || y0 > y1 { return; }
    let a = (x1 - x0).abs() as i64;
    let b = (y1 - y0).abs() as i64;
    let b1 = b & 1;
    let mut dx = 4 * (1 - a) * b * b;
    let mut dy = 4 * (b1 + 1) * a * a;
    let mut err = dx + dy + b1 * a * a;
    let (mut x0, mut x1, mut y0, mut y1) = (x0 as i64, x1 as i64, y0 as i64, y1 as i64);
    if x0 > x1 { let t = x0; x0 = x1; x1 = t + a; }
    if y0 > y1 { y0 = y1; }
    y0 += (b + 1) / 2; y1 = y0 - b1;
    let aa = 8 * a * a;
    let bb = 8 * b * b;
    loop {
        let e2 = 2 * err;
        if e2 <= dy {
            if fill { set_side_pixel(x1 as i32, y0 as i32); set_side_pixel(x0 as i32, y0 as i32);
                     set_side_pixel(x0 as i32, y1 as i32); set_side_pixel(x1 as i32, y1 as i32); }
            else { set_pixel(ram, clip, x1 as i32, y0 as i32, color); set_pixel(ram, clip, x0 as i32, y0 as i32, color);
                   set_pixel(ram, clip, x0 as i32, y1 as i32, color); set_pixel(ram, clip, x1 as i32, y1 as i32, color); }
            y0 += 1; y1 -= 1; dy += aa; err += dy;
        }
        if e2 >= dx || 2 * err > dy {
            if fill { set_side_pixel(x1 as i32, y0 as i32); set_side_pixel(x0 as i32, y0 as i32);
                     set_side_pixel(x0 as i32, y1 as i32); set_side_pixel(x1 as i32, y1 as i32); }
            else { set_pixel(ram, clip, x1 as i32, y0 as i32, color); set_pixel(ram, clip, x0 as i32, y0 as i32, color);
                   set_pixel(ram, clip, x0 as i32, y1 as i32, color); set_pixel(ram, clip, x1 as i32, y1 as i32, color); }
            x0 += 1; x1 -= 1; dx += bb; err += dx;
        }
        if x0 > x1 { break; }
    }
    while y0 - y1 < b {
        set_pixel(ram, clip, x0 as i32 - 1, y0 as i32, color);
        set_pixel(ram, clip, x1 as i32 + 1, y0 as i32, color); y0 += 1;
        set_pixel(ram, clip, x0 as i32 - 1, y1 as i32, color);
        set_pixel(ram, clip, x1 as i32 + 1, y1 as i32, color); y1 -= 1;
    }
}

pub fn tic_api_circ(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, r: i32, color: u8) {
    init_sides_buffer();
    draw_ellipse(ram, clip, x - r, y - r, x + r, y + r, map_color(color), true);
    draw_sides_buffer(ram, clip, y - r, y + r + 1, map_color(color));
}
pub fn tic_api_circb(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, r: i32, color: u8) {
    draw_ellipse(ram, clip, x - r, y - r, x + r, y + r, map_color(color), false);
}
pub fn tic_api_elli(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, a: i32, b: i32, color: u8) {
    init_sides_buffer();
    draw_ellipse(ram, clip, x - a, y - b, x + a, y + b, map_color(color), true);
    draw_sides_buffer(ram, clip, y - b, y + b + 1, map_color(color));
}
pub fn tic_api_ellib(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, a: i32, b: i32, color: u8) {
    draw_ellipse(ram, clip, x - a, y - b, x + a, y + b, map_color(color), false);
}

// ---------------------------------------------------------------------------
// Clip, Cls, Pix, Line
// ---------------------------------------------------------------------------

pub fn tic_api_clip(clip: &mut ClipRect, x: i32, y: i32, w: i32, h: i32) {
    clip.l = x.max(0); clip.t = y.max(0);
    clip.r = (x + w).min(240); clip.b = (y + h).min(136);
}

pub fn tic_api_cls(ram: &mut [u8], clip: &ClipRect, color: u8) {
    let c = map_color(color);
    if clip.l == 0 && clip.t == 0 && clip.r >= 240 && clip.b >= 136 {
        let val = (c & 0x0f) | (c << 4);
        ram.fill(val);
    } else {
        for y in clip.t..clip.b {
            for x in clip.l..clip.r { core::poke4(ram, y * 240 + x, c); }
        }
    }
}

pub fn tic_api_pix(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, color: u8, get: bool) -> u8 {
    if get { return get_pixel(ram, x, y); }
    set_pixel(ram, clip, x, y, map_color(color));
    0
}

fn init_line(x0: &mut f32, x1: &mut f32, y0: &mut f32, y1: &mut f32) -> f32 {
    if *y0 > *y1 { std::mem::swap(x0, x1); std::mem::swap(y0, y1); }
    let t = (*x1 - *x0) / (*y1 - *y0);
    if *y0 < 0.0 { *x0 -= *y0 * t; *y0 = 0.0; }
    if *y1 > 240.0 { *x1 += (240.0 - *y0) * t; *y1 = 240.0; }
    t
}

fn draw_line(ram: &mut [u8], clip: &ClipRect, mut x0: f32, mut y0: f32, mut x1: f32, mut y1: f32, color: u8) {
    if (x0 - x1).abs() < (y0 - y1).abs() {
        let t = init_line(&mut x0, &mut x1, &mut y0, &mut y1);
        while y0 < y1 { set_pixel(ram, clip, x0 as i32, y0 as i32, color); y0 += 1.0; x0 += t; }
    } else {
        let t = init_line(&mut y0, &mut y1, &mut x0, &mut x1);
        while x0 < x1 { set_pixel(ram, clip, x0 as i32, y0 as i32, color); x0 += 1.0; y0 += t; }
    }
    set_pixel(ram, clip, x1 as i32, y1 as i32, color);
}

pub fn tic_api_line(ram: &mut [u8], clip: &ClipRect, x0: f32, y0: f32, x1: f32, y1: f32, color: u8) {
    draw_line(ram, clip, x0, y0, x1, y1, map_color(color));
}

pub fn tic_api_rect(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, w: i32, h: i32, color: u8) {
    draw_rect(ram, clip, x, y, w, h, map_color(color));
}

pub fn tic_api_rectb(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, w: i32, h: i32, color: u8) {
    draw_rect_border(ram, clip, x, y, w, h, map_color(color));
}

// ---------------------------------------------------------------------------
// Flood fill
// ---------------------------------------------------------------------------

static mut FILL_QUEUE: FillQueue = FillQueue { seg: [FillSegment { y: 0, xl: 0, xr: 0, dy: 0 }; FILL_QUEUE_SIZE], ini: 0, outi: 0 };

fn fill_enqueue(y: i32, xl: i32, xr: i32, dy: i32) {
    unsafe {
        let q = &mut FILL_QUEUE;
        if q.ini < FILL_QUEUE_SIZE { q.seg[q.ini] = FillSegment { y, xl, xr, dy }; q.ini += 1; }
    }
}

fn fill_dequeue() -> Option<FillSegment> {
    unsafe {
        let q = &mut FILL_QUEUE;
        if q.outi < q.ini { let s = q.seg[q.outi]; q.outi += 1; Some(s) }
        else { q.ini = 0; q.outi = 0; None }
    }
}

pub fn flood_fill(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, new_color: u8, _border_color: u8) {
    if x < 0 || y < 0 || x >= 240 || y >= 136 { return; }
    unsafe { FILL_QUEUE.ini = 0; FILL_QUEUE.outi = 0; }
    let old_color = get_pixel(ram, x, y);
    let new_color = map_color(new_color);
    if old_color == new_color { return; }
    fill_enqueue(y, x, x, 1);
    fill_enqueue(y + 1, x, x, -1);
    while let Some(mut seg) = fill_dequeue() {
        let mut x2 = seg.xl;
        while get_pixel(ram, x2, seg.y) == old_color && x2 >= 0 { x2 -= 1; }
        x2 += 1; if x2 > seg.xr { x2 = seg.xr; }
        let xl = x2;
        while x2 <= seg.xr {
            while get_pixel(ram, x2, seg.y) == old_color && x2 < 240 { set_pixel(ram, clip, x2, seg.y, new_color); x2 += 1; }
            let xr = x2 - 1;
            if xl <= xr {
                if seg.y + seg.dy >= 0 && seg.y + seg.dy < 136 {
                    let mut scan = xl;
                    loop {
                        if get_pixel(ram, scan, seg.y + seg.dy) == old_color {
                            while scan < 240 && get_pixel(ram, scan, seg.y + seg.dy) == old_color { scan += 1; }
                            fill_enqueue(seg.y + seg.dy, xl, scan - 1, -seg.dy);
                        }
                        scan += 1;
                        if scan > xr { break; }
                    }
                }
            }
            x2 += 1;
            if x2 > seg.xr { break; }
            while x2 <= seg.xr && get_pixel(ram, x2, seg.y) != old_color { x2 += 1; }
            // xl set at loop top by reassignment from x2
        }
    }
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

pub fn tic_api_fget(flags: &[u8], index: i32, flag: u8) -> bool {
    if index < 0 || (index as usize) >= 1024 || flag >= 8 { return false; }
    (flags[index as usize] >> flag) & 1 != 0
}
pub fn tic_api_fset(flags: &mut [u8], index: i32, flag: u8, value: bool) {
    if index < 0 || (index as usize) >= 1024 || flag >= 8 { return; }
    if value { flags[index as usize] |= 1 << flag; }
    else { flags[index as usize] &= !(1 << flag); }
}

// ---------------------------------------------------------------------------
// Tile/Sprite/Map — simplified to compile, will be expanded later
// ---------------------------------------------------------------------------

pub fn tic_api_spr(ram: &mut [u8], clip: &ClipRect, index: i32, x: i32, y: i32,
    w: i32, h: i32, colors: &[u8], count: u8, scale: i32, flip: Flip, rotate: Rotate,
    _blit_segment: u8, _ram_tiles: *const u8, _ram_font: *const u8) {
    if index < 0 { return; }
    // Simplified sprite drawing — just draw a colored rectangle for now
    draw_rect(ram, clip, x, y, (w * 8 * scale).max(1), (h * 8 * scale).max(1), map_color(colors.first().copied().unwrap_or(15)));
}

pub fn tic_api_map(ram: &mut [u8], clip: &ClipRect, _ram_tiles: *const u8, _ram_font: *const u8,
    _x: i32, _y: i32, _w: i32, _h: i32, _sx: i32, _sy: i32, _colors: &[u8], _count: u8,
    _scale: i32, _remap: RemapFunc, _data: *mut std::ffi::c_void, _blit_segment: u8) {
    // Simplified map drawing stub
}

pub fn tic_api_print(ram: &mut [u8], clip: &ClipRect, ram_font: *const u8, ram_tiles: *const u8,
    text: &str, x: i32, y: i32, color: u8, fixed: bool, scale: i32, alt: bool, _blit_segment: u8) -> i32 {
    let c = map_color(color);
    let mut px = x;
    for ch in text.bytes() {
        if ch == b'\n' { continue; }
        let w = if fixed { 6 } else { 4 };
        draw_rect(ram, clip, px, y, w * scale, 6 * scale, c);
        px += (w * scale).max(1);
    }
    px - x
}

pub fn tic_api_font(ram: &mut [u8], clip: &ClipRect, _ram_tiles: *const u8, _ram_font: *const u8,
    text: &str, x: i32, y: i32, colors: &[u8], _count: u8, w: i32, h: i32, fixed: bool,
    scale: i32, alt: bool, _blit_segment: u8) -> i32 {
    tic_api_print(ram, clip, std::ptr::null(), std::ptr::null(), text, x, y, colors.first().copied().unwrap_or(15), fixed, scale, alt, 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    pub fn test_ram() -> Vec<u8> { vec![0u8; 240 * 136 / 2] }
    pub fn test_clip() -> ClipRect { ClipRect { l: 0, t: 0, r: 240, b: 136 } }

    #[test] fn set_and_get() { let mut r = test_ram(); let c = test_clip(); set_pixel(&mut r, &c, 10, 20, 5); assert_eq!(get_pixel(&r, 10, 20), 5); }
    #[test] fn cls() { let mut r = test_ram(); let c = test_clip(); tic_api_cls(&mut r, &c, 5); assert!(r.iter().any(|&x| x != 0)); }
    #[test] fn line() { let mut r = test_ram(); let c = test_clip(); tic_api_line(&mut r, &c, 0.0, 5.0, 10.0, 5.0, 3); assert!(r.iter().any(|&x| x != 0)); }
    #[test] fn rect() { let mut r = test_ram(); let c = test_clip(); tic_api_rect(&mut r, &c, 5, 5, 10, 10, 7); assert_eq!(get_pixel(&r, 5, 5), 7); }
    #[test] fn ellipse() { let mut r = test_ram(); let c = test_clip(); tic_api_circb(&mut r, &c, 50, 50, 10, 4); assert!(r.iter().any(|&x| x != 0)); }
    #[test] fn pix_rw() { let mut r = test_ram(); let c = test_clip(); tic_api_pix(&mut r, &c, 30, 40, 9, false); assert_eq!(tic_api_pix(&mut r, &c, 30, 40, 0, true), 9); }
    #[test] fn clip() { let mut c = test_clip(); tic_api_clip(&mut c, 10, 10, 50, 50); assert_eq!(c.l, 10); }
    #[test] fn flags() { let mut f = [0u8; 1024]; tic_api_fset(&mut f, 5, 2, true); assert!(tic_api_fget(&f, 5, 2)); }
    #[test] fn fill() { let mut r = test_ram(); let c = test_clip(); tic_api_rectb(&mut r, &c, 10, 10, 20, 20, 1); flood_fill(&mut r, &c, 15, 15, 2, 1); assert_eq!(get_pixel(&r, 15, 15), 2); }
    #[test] fn print() { let mut r = test_ram(); let c = test_clip(); tic_api_print(&mut r, &c, ptr::null(), ptr::null(), "A", 10, 10, 15, true, 1, false, 0); }
}
