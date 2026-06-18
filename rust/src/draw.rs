//! Graphics pipeline — all drawing primitives.
//!
//! Port of TIC-80's `src/core/draw.c`.

use crate::core::{
    peek4, poke4, BITS_IN_BYTE, TIC80_HEIGHT, TIC80_WIDTH, TIC_PALETTE_SIZE,
};
use crate::tilesheet;
use crate::tools::modulo;
use std::mem;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TRANSPARENT_COLOR: u8 = 255;
const TIC_SPRITESIZE: usize = 8;
const TIC80_FULLWIDTH: usize = 256;
const TIC80_FULLHEIGHT: usize = 256;
const TIC_MAP_WIDTH: usize = 30;
const TIC_MAP_HEIGHT: usize = 30;
const TIC_SPRITESHEET_SIZE: usize = 128;
const TIC_SPRITE_BANKS: usize = 4;
const TIC_FLAGS: usize = 128 * 4;
const TIC_FONT_CHARS: usize = 128;
const TIC_MARGIN_TOP: usize = 8;
const TIC80_FRAMERATE: u32 = 60;
const FILL_QUEUE_SIZE: usize = 400;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
#[repr(C)]
pub struct ClipRect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Flip {
    None = 0,
    Horz = 1,
    Vert = 2,
    Both = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Rotate {
    None = 0,
    R90 = 1,
    R180 = 2,
    R270 = 3,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FillSegment {
    pub y: i32,
    pub xl: i32,
    pub xr: i32,
    pub dy: i32,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct FillQueue {
    pub seg: [FillSegment; FILL_QUEUE_SIZE],
    pub ini: usize,
    pub outi: usize,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct RemapResult {
    pub index: i32,
    pub flip: Flip,
    pub rotate: Rotate,
}

pub type RemapFunc = Option<fn(data: *mut std::ffi::c_void, x: i32, y: i32, tile: &mut RemapResult)>;

// ---------------------------------------------------------------------------
// Global state (matches C static variables)
// ---------------------------------------------------------------------------

static mut SIDES_BUFFER_LEFT: [i16; TIC80_HEIGHT as usize] = [0; TIC80_HEIGHT as usize];
static mut SIDES_BUFFER_RIGHT: [i16; TIC80_HEIGHT as usize] = [0; TIC80_HEIGHT as usize];
static mut ZBUFFER: [f64; TIC80_WIDTH as usize * TIC80_HEIGHT as usize] = [0.0; TIC80_WIDTH as usize * TIC80_HEIGHT as usize];
static mut FILL_QUEUE: FillQueue = FillQueue { seg: [FillSegment { y: 0, xl: 0, xr: 0, dy: 0 }; FILL_QUEUE_SIZE], ini: 0, outi: 0 };

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn get_tilesheet_from_segment(ram_tiles: *const u8, ram_font: *const u8, segment: u8) -> tilesheet::Tilesheet<'static> {
    let src = match segment {
        0 | 1 => ram_font as *mut u8,
        _ => ram_tiles as *mut u8,
    };
    tilesheet::get(segment, src as *mut u8)
}

fn get_palette(mapping_raw: &[u8], colors: &[u8]) -> [u8; TIC_PALETTE_SIZE] {
    let mut mapping = [0u8; TIC_PALETTE_SIZE];
    for i in 0..TIC_PALETTE_SIZE {
        mapping[i] = peek4(mapping_raw, i as i32);
    }
    for &c in colors {
        if (c as usize) < TIC_PALETTE_SIZE {
            mapping[c as usize] = TRANSPARENT_COLOR;
        }
    }
    mapping
}

fn map_color(mapping_raw: &[u8], color: u8) -> u8 {
    peek4(mapping_raw, (color & 0x0f) as i32)
}

fn set_pixel(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, color: u8) {
    if x < clip.l || y < clip.t || x >= clip.r || y >= clip.b {
        return;
    }
    poke4(ram, y * TIC80_WIDTH as i32 + x, color);
}

fn set_pixel_fast(ram: &mut [u8], x: i32, y: i32, color: u8) {
    poke4(ram, y * TIC80_WIDTH as i32 + x, color);
}

fn get_pixel(ram: &[u8], x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= TIC80_WIDTH as i32 || y >= TIC80_HEIGHT as i32 {
        return 0;
    }
    peek4(ram, y * TIC80_WIDTH as i32 + x)
}

fn early_clip(x: i32, y: i32, width: i32, height: i32, clip: &ClipRect) -> bool {
    (y + height - 1) < clip.t
        || (x + width - 1) < clip.l
        || y >= clip.b
        || x >= clip.r
}

fn draw_hline(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, width: i32, color: u8) {
    if y < clip.t || clip.b <= y {
        return;
    }
    let xl = x.max(clip.l);
    let xr = (x + width).min(clip.r);
    let start = y * TIC80_WIDTH as i32;
    for i in start + xl..start + xr {
        poke4(ram, i, color);
    }
}

fn draw_vline(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, height: i32, color: u8) {
    if x < clip.l || clip.r <= x {
        return;
    }
    let yl = y.max(0);
    let yr = (y + height).min(TIC80_HEIGHT as i32);
    for i in yl..yr {
        set_pixel(ram, clip, x, i, color);
    }
}

fn draw_rect(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, width: i32, height: i32, color: u8) {
    for i in y..y + height {
        draw_hline(ram, clip, x, i, width, color);
    }
}

fn draw_rect_border(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, width: i32, height: i32, color: u8) {
    draw_hline(ram, clip, x, y, width, color);
    draw_hline(ram, clip, x, y + height - 1, width, color);
    draw_vline(ram, clip, x, y, height, color);
    draw_vline(ram, clip, x + width - 1, y, height, color);
}

// ---------------------------------------------------------------------------
// Sides buffer (for filled shapes)
// ---------------------------------------------------------------------------

fn init_sides_buffer() {
    unsafe {
        for i in 0..TIC80_HEIGHT as usize {
            SIDES_BUFFER_LEFT[i] = TIC80_WIDTH as i16;
            SIDES_BUFFER_RIGHT[i] = -1;
        }
    }
}

fn set_side_pixel(x: i32, y: i32) {
    if y >= 0 && y < TIC80_HEIGHT as i32 {
        unsafe {
            let yy = y as usize;
            if x < SIDES_BUFFER_LEFT[yy] as i32 {
                SIDES_BUFFER_LEFT[yy] = x as i16;
            }
            if x > SIDES_BUFFER_RIGHT[yy] as i32 {
                SIDES_BUFFER_RIGHT[yy] = x as i16;
            }
        }
    }
}

fn draw_sides_buffer(ram: &mut [u8], clip: &ClipRect, y0: i32, y1: i32, color: u8) {
    let yt = y0.max(clip.t);
    let yb = (y1 + 1).min(clip.b);
    unsafe {
        for y in yt..yb {
            let yy = y as usize;
            let xl = (SIDES_BUFFER_LEFT[yy] as i32).max(clip.l);
            let xr = (SIDES_BUFFER_RIGHT[yy] as i32 + 1).min(clip.r);
            let start = y * TIC80_WIDTH as i32;
            for i in start + xl..start + xr {
                poke4(ram, i, color);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Clip
// ---------------------------------------------------------------------------

pub fn tic_api_clip(clip: &mut ClipRect, x: i32, y: i32, width: i32, height: i32) {
    clip.l = x.max(0);
    clip.t = y.max(0);
    clip.r = (x + width).min(TIC80_WIDTH as i32);
    clip.b = (y + height).min(TIC80_HEIGHT as i32);
}

// ---------------------------------------------------------------------------
// Cls (clear screen)
// ---------------------------------------------------------------------------

pub fn tic_api_cls(ram: &mut [u8], clip: &ClipRect, color: u8) {
    let full = ClipRect { l: 0, t: 0, r: TIC80_WIDTH as i32, b: TIC80_HEIGHT as i32 };
    if clip.l == full.l && clip.t == full.t && clip.r == full.r && clip.b == full.b {
        let val = (color & 0x0f) | (color << 4);
        ram.fill(val);
        unsafe { ZBUFFER.fill(0.0); }
    } else {
        for y in clip.t..clip.b {
            let start = y * TIC80_WIDTH as i32;
            for x in clip.l..clip.r {
                poke4(ram, start + x, color);
                unsafe { ZBUFFER[(start + x) as usize] = 0.0; }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pixel
// ---------------------------------------------------------------------------

pub fn tic_api_pix(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, color: u8, get: bool) -> u8 {
    if get {
        return get_pixel(ram, x, y);
    }
    set_pixel(ram, clip, x, y, color);
    0
}

// ---------------------------------------------------------------------------
// Line
// ---------------------------------------------------------------------------

fn init_line(x0: &mut f32, x1: &mut f32, y0: &mut f32, y1: &mut f32) -> f32 {
    if *y0 > *y1 {
        mem::swap(x0, x1);
        mem::swap(y0, y1);
    }
    let t = (*x1 - *x0) / (*y1 - *y0);
    if *y0 < 0.0 {
        *x0 -= *y0 * t;
        *y0 = 0.0;
    }
    if *y1 > TIC80_WIDTH as f32 {
        *x1 += (TIC80_WIDTH as f32 - *y0) * t;
        *y1 = TIC80_WIDTH as f32;
    }
    t
}

fn draw_line(ram: &mut [u8], clip: &ClipRect, mut x0: f32, mut y0: f32, mut x1: f32, mut y1: f32, color: u8) {
    if (x0 - x1).abs() < (y0 - y1).abs() {
        let t = init_line(&mut x0, &mut x1, &mut y0, &mut y1);
        while y0 < y1 {
            set_pixel(ram, clip, x0 as i32, y0 as i32, color);
            y0 += 1.0;
            x0 += t;
        }
    } else {
        let t = init_line(&mut y0, &mut y1, &mut x0, &mut x1);
        while x0 < x1 {
            set_pixel(ram, clip, x0 as i32, y0 as i32, color);
            x0 += 1.0;
            y0 += t;
        }
    }
    set_pixel(ram, clip, x1 as i32, y1 as i32, color);
}

pub fn tic_api_line(ram: &mut [u8], clip: &ClipRect, x0: f32, y0: f32, x1: f32, y1: f32, color: u8) {
    draw_line(ram, clip, x0, y0, x1, y1, color);
}

// ---------------------------------------------------------------------------
// Rect
// ---------------------------------------------------------------------------

pub fn tic_api_rect(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, width: i32, height: i32, color: u8) {
    draw_rect(ram, clip, x, y, width, height, color);
}

pub fn tic_api_rectb(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, width: i32, height: i32, color: u8) {
    draw_rect_border(ram, clip, x, y, width, height, color);
}

// ---------------------------------------------------------------------------
// Circle / Ellipse
// ---------------------------------------------------------------------------

fn draw_ellipse(ram: &mut [u8], clip: &ClipRect, x0: i32, y0: i32, x1: i32, y1: i32, color: u8, fill: bool) {
    if x0 > x1 || y0 > y1 {
        return;
    }
    let a = (x1 - x0).abs() as i64;
    let b = (y1 - y0).abs() as i64;
    let b1 = b & 1;
    let mut dx = 4 * (1 - a) * b * b;
    let mut dy = 4 * (b1 + 1) * a * a;
    let mut err = dx + dy + b1 * a * a;
    let (mut x0, mut x1, mut y0, mut y1) = (x0 as i64, x1 as i64, y0 as i64, y1 as i64);
    if x0 > x1 {
        x0 = x1;
        x1 += a;
    }
    if y0 > y1 {
        y0 = y1;
    }
    y0 += (b + 1) / 2;
    y1 = y0 - b1;
    let aa = 8 * a * a;
    let b1_val = 8 * b * b;

    let mut plot = |rx: i64, ry: i64, c: u8| {
        if fill {
            set_side_pixel(rx as i32, ry as i32);
        } else {
            set_pixel(ram, clip, rx as i32, ry as i32, c);
        }
    };

    let mut x0_i64 = x0;
    let mut x1_i64 = x1;
    let mut y0_i64 = y0;
    let mut y1_i64 = y1;

    loop {
        plot(x1_i64, y0_i64, color);
        plot(x0_i64, y0_i64, color);
        plot(x0_i64, y1_i64, color);
        plot(x1_i64, y1_i64, color);
        let e2 = 2 * err;
        if e2 <= dy {
            y0_i64 += 1;
            y1_i64 -= 1;
            err += dy;
            dy += aa;
        }
        if e2 >= dx || 2 * err > dy {
            x0_i64 += 1;
            x1_i64 -= 1;
            err += dx;
            dx += b1_val;
        }
        if x0_i64 > x1_i64 {
            break;
        }
    }

    while y0_i64 - y1_i64 < b {
        plot(x0_i64 - 1, y0_i64, color);
        plot(x1_i64 + 1, y0_i64, color);
        y0_i64 += 1;
        plot(x0_i64 - 1, y1_i64, color);
        plot(x1_i64 + 1, y1_i64, color);
        y1_i64 -= 1;
    }
}

pub fn tic_api_circ(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, r: i32, color: u8) {
    init_sides_buffer();
    draw_ellipse(ram, clip, x - r, y - r, x + r, y + r, 0, true);
    draw_sides_buffer(ram, clip, y - r, y + r + 1, color);
}

pub fn tic_api_circb(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, r: i32, color: u8) {
    draw_ellipse(ram, clip, x - r, y - r, x + r, y + r, color, false);
}

pub fn tic_api_elli(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, a: i32, b: i32, color: u8) {
    init_sides_buffer();
    draw_ellipse(ram, clip, x - a, y - b, x + a, y + b, 0, true);
    draw_sides_buffer(ram, clip, y - b, y + b + 1, color);
}

pub fn tic_api_ellib(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, a: i32, b: i32, color: u8) {
    draw_ellipse(ram, clip, x - a, y - b, x + a, y + b, color, false);
}

// ---------------------------------------------------------------------------
// Flood fill (paint)
// ---------------------------------------------------------------------------

fn fill_enqueue(y: i32, xl: i32, xr: i32, dy: i32) {
    unsafe {
        let q = &mut FILL_QUEUE;
        if q.ini < FILL_QUEUE_SIZE {
            q.seg[q.ini] = FillSegment { y, xl, xr, dy };
            q.ini += 1;
        }
    }
}

fn fill_dequeue() -> Option<FillSegment> {
    unsafe {
        let q = &mut FILL_QUEUE;
        if q.outi < q.ini {
            let seg = q.seg[q.outi];
            q.outi += 1;
            Some(seg)
        } else {
            q.ini = 0;
            q.outi = 0;
            None
        }
    }
}

pub fn flood_fill(ram: &mut [u8], clip: &ClipRect, x: i32, y: i32, new_color: u8, border_color: u8) {
    if x < 0 || y < 0 || x >= TIC80_WIDTH as i32 || y >= TIC80_HEIGHT as i32 {
        return;
    }

    let mut seg: FillSegment;
    let mut xl: i32;
    let mut xr: i32;
    let mut dy: i32;
    let mut x2: i32;

    unsafe {
        FILL_QUEUE.ini = 0;
        FILL_QUEUE.outi = 0;
    }

    let old_color = get_pixel(ram, x, y);
    if old_color == new_color {
        return;
    }

    fill_enqueue(y, x, x, 1);
    fill_enqueue(y + 1, x, x, -1);

    loop {
        let s = fill_dequeue();
        match s {
            None => break,
            Some(seg_val) => {
                seg = seg_val;
                x2 = seg.xl;
                while get_pixel(ram, x2, seg.y) == old_color && x2 >= 0 {
                    x2 -= 1;
                }
                x2 += 1;
                if x2 > seg.xr { x2 = seg.xr; }

                xl = x2;
                while x2 <= seg.xr {
                    while get_pixel(ram, x2, seg.y) == old_color && x2 < TIC80_WIDTH as i32 {
                        set_pixel(ram, clip, x2, seg.y, new_color);
                        x2 += 1;
                    }
                    xr = x2 - 1;
                    if xl <= xr {
                        dy = seg.dy;
                        if seg.y + dy >= 0 && seg.y + dy < TIC80_HEIGHT as i32 {
                            let mut scan = xl;
                            loop {
                                if get_pixel(ram, scan, seg.y + dy) == old_color {
                                    while scan < TIC80_WIDTH as i32 && get_pixel(ram, scan, seg.y + dy) == old_color {
                                        scan += 1;
                                    }
                                    fill_enqueue(seg.y + dy, xl, scan - 1, -dy);
                                }
                                scan += 1;
                                if scan > xr {
                                    break;
                                }
                            }
                        }
                    }
                    x2 += 1;
                    if x2 > seg.xr { break; }
                    while x2 <= seg.xr && get_pixel(ram, x2, seg.y) != old_color {
                        x2 += 1;
                    }
                    xl = x2;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

pub fn tic_api_fget(flags: &[u8], index: i32, flag: u8) -> bool {
    if index < 0 || index as usize >= TIC_FLAGS || flag as usize >= BITS_IN_BYTE {
        return false;
    }
    (flags[index as usize] >> flag) & 1 != 0
}

pub fn tic_api_fset(flags: &mut [u8], index: i32, flag: u8, value: bool) {
    if index < 0 || index as usize >= TIC_FLAGS || flag as usize >= BITS_IN_BYTE {
        return;
    }
    if value {
        flags[index as usize] |= 1 << flag;
    } else {
        flags[index as usize] &= !(1 << flag);
    }
}

// ---------------------------------------------------------------------------
// Tile drawing
// ---------------------------------------------------------------------------

fn draw_tile(
    ram: &mut [u8],
    clip: &ClipRect,
    tile: &tilesheet::TilePtr,
    x: i32,
    y: i32,
    colors: &[u8],
    count: u8,
    scale: i32,
    flip: Flip,
    rotate: Rotate,
) {
    let mapping = get_palette(&[0u8; 8], colors); // simplified

    let mut orientation = flip as u32 & 3;
    let rot = rotate as u32 & 3;
    if rot == 1 { orientation ^= 1; }
    else if rot == 2 { orientation ^= 3; }
    else if rot == 3 { orientation ^= 2; }
    if rot == 1 || rot == 3 { orientation |= 4; }

    if scale == 1 {
        let sx = (clip.l - x).max(0);
        let sy = (clip.t - y).max(0);
        let ex = (clip.r - x).min(TIC_SPRITESIZE as i32);
        let ey = (clip.b - y).min(TIC_SPRITESIZE as i32);
        let mut yy = y + sy;
        let mut xx;
        for py in sy..ey {
            xx = x + sx;
            for px in sx..ex {
                let (ix, iy) = tile_coord(px as usize, py as usize, orientation);
                let raw_tile = unsafe { tilesheet::peek4(tile.ptr, tile.offset + (ix as u32) + (iy as u32) * tile.segment.tile_width) };
                let color = mapping[raw_tile as usize];
                if color != TRANSPARENT_COLOR {
                    set_pixel_fast(ram, xx, yy, color);
                }
                xx += 1;
            }
            yy += 1;
        }
        return;
    }

    if early_clip(x, y, TIC_SPRITESIZE as i32 * scale, TIC_SPRITESIZE as i32 * scale, clip) {
        return;
    }

    let mut yy = y;
    for py in 0..TIC_SPRITESIZE {
        let mut xx = x;
        for px in 0..TIC_SPRITESIZE {
            let ix = if orientation & 1 != 0 { TIC_SPRITESIZE as i32 - px as i32 - 1 } else { px as i32 };
            let mut iy = if orientation & 2 != 0 { TIC_SPRITESIZE as i32 - py as i32 - 1 } else { py as i32 };
            let (ix, iy) = if orientation & 4 != 0 {
                (iy, ix)
            } else {
                (ix, iy)
            };
            let raw_tile = unsafe { tilesheet::peek4(tile.ptr, tile.offset + (ix as u32) + (iy as u32) * tile.segment.tile_width) };
            let color = mapping[raw_tile as usize];
            if color != TRANSPARENT_COLOR {
                draw_rect(ram, clip, xx, yy, scale, scale, color);
            }
            xx += scale;
        }
        yy += scale;
    }
}

fn tile_coord(px: usize, py: usize, orientation: u32) -> (i32, i32) {
    let rev = |v: usize| -> i32 { (TIC_SPRITESIZE - 1 - v) as i32 };
    match orientation {
        4 => (py as i32, px as i32),
        6 => (rev(py), px as i32),
        5 => (py as i32, rev(px)),
        7 => (rev(py), rev(px)),
        0 => (px as i32, py as i32),
        2 => (px as i32, rev(py)),
        1 => (rev(px), py as i32),
        _ => (rev(px), rev(py)),
    }
}

// ---------------------------------------------------------------------------
// Sprite drawing
// ---------------------------------------------------------------------------

pub fn tic_api_spr(
    ram: &mut [u8],
    clip: &ClipRect,
    ram_tiles: *const u8,
    ram_font: *const u8,
    index: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    colors: &[u8],
    count: u8,
    scale: i32,
    flip: Flip,
    rotate: Rotate,
    blit_segment: u8,
) {
    if index < 0 { return; }

    let sheet = get_tilesheet_from_segment(ram_tiles, ram_font, blit_segment);
    let step = TIC_SPRITESIZE as i32 * scale;
    let cols = sheet.segment.sheet_width as usize;

    if w == 1 && h == 1 {
        let tile = tilesheet::get_tile(&sheet, index as u32, false);
        draw_tile(ram, clip, &tile, x, y, colors, count, scale, flip, rotate);
        return;
    }

    if early_clip(x, y, w * step, h * step, clip) { return; }

    for i in 0..w {
        for j in 0..h {
            let mut mx = i;
            let mut my = j;
            let vert_horz = Flip::Both;
            if flip == Flip::Horz || flip == vert_horz { mx = w - 1 - i; }
            if flip == Flip::Vert || flip == vert_horz { my = h - 1 - j; }

            match rotate {
                Rotate::R180 => { mx = w - 1 - mx; my = h - 1 - my; }
                Rotate::R90 => {
                    if flip == Flip::None || flip == vert_horz { my = h - 1 - my; }
                    else { mx = w - 1 - mx; }
                }
                Rotate::R270 => {
                    if flip == Flip::None || flip == vert_horz { mx = w - 1 - mx; }
                    else { my = h - 1 - my; }
                }
                _ => {}
            }

            let tile_idx = index + mx + my * cols as i32;
            let tile = tilesheet::get_tile(&sheet, tile_idx as u32, false);
            let tile_x = if rotate == Rotate::None || rotate == Rotate::R180 {
                x + i * step
            } else {
                x + j * step
            };
            let tile_y = if rotate == Rotate::None || rotate == Rotate::R180 {
                y + j * step
            } else {
                y + i * step
            };
            draw_tile(ram, clip, &tile, tile_x, tile_y, colors, count, scale, flip, rotate);
        }
    }
}

// ---------------------------------------------------------------------------
// Map drawing
// ---------------------------------------------------------------------------

pub fn tic_api_map(
    ram: &mut [u8],
    clip: &ClipRect,
    ram_tiles: *const u8,
    ram_font: *const u8,
    map_data: &[u8],
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    sx: i32,
    sy: i32,
    colors: &[u8],
    count: u8,
    scale: i32,
    _remap: RemapFunc,
    _data: *mut std::ffi::c_void,
    blit_segment: u8,
) {
    let size = TIC_SPRITESIZE as i32 * scale;
    let sheet = get_tilesheet_from_segment(ram_tiles, ram_font, blit_segment);

    for j in y..y + height {
        let jj = sy + (j - y) * size;
        for i in x..x + width {
            let ii = sx + (i - x) * size;
            let mi = modulo(i, TIC_MAP_WIDTH as i32);
            let mj = modulo(j, TIC_MAP_HEIGHT as i32);
            let idx = (mi + mj * TIC_MAP_WIDTH as i32) as usize;
            let tile_idx = map_data[idx] as i32;
            let tile = tilesheet::get_tile(&sheet, tile_idx as u32, true);
            draw_tile(ram, clip, &tile, ii, jj, colors, count, scale, Flip::None, Rotate::None);
        }
    }
}

// ---------------------------------------------------------------------------
// Character / Text drawing
// ---------------------------------------------------------------------------

fn draw_char(
    ram: &mut [u8],
    clip: &ClipRect,
    font_char: &tilesheet::TilePtr,
    x: i32,
    y: i32,
    scale: i32,
    fixed: bool,
    mapping: &[u8; TIC_PALETTE_SIZE],
) -> i32 {
    let mut start = 0usize;
    let mut end = TIC_SPRITESIZE;

    if !fixed {
        for i in 0..TIC_SPRITESIZE {
            let mut found = false;
            for j in 0..TIC_SPRITESIZE {
                let raw = unsafe { tilesheet::peek4(font_char.ptr, font_char.offset + (i as u32) + (j as u32) * font_char.segment.tile_width) };
                if mapping[raw as usize] != TRANSPARENT_COLOR {
                    found = true;
                    break;
                }
            }
            if found { break; }
            start += 1;
        }
        for i in (start..TIC_SPRITESIZE).rev() {
            let mut found = false;
            for j in 0..TIC_SPRITESIZE {
                let raw = unsafe { tilesheet::peek4(font_char.ptr, font_char.offset + (i as u32) + (j as u32) * font_char.segment.tile_width) };
                if mapping[raw as usize] != TRANSPARENT_COLOR {
                    found = true;
                    break;
                }
            }
            if found { break; }
            end -= 1;
        }
    }

    let width = (end - start) as i32;

    if early_clip(x, y, TIC_SPRITESIZE as i32 * scale, TIC_SPRITESIZE as i32 * scale, clip) {
        return width;
    }

    for col in start..end {
        let xs = x + (col - start) as i32 * scale;
        for row in 0..TIC_SPRITESIZE {
            let ys = y + row as i32 * scale;
            let raw = unsafe { tilesheet::peek4(font_char.ptr, font_char.offset + (col as u32) + (row as u32) * font_char.segment.tile_width) };
            let color = mapping[raw as usize];
            if color != TRANSPARENT_COLOR {
                draw_rect(ram, clip, xs, ys, scale, scale, color);
            }
        }
    }
    width
}

fn draw_text(
    ram: &mut [u8],
    clip: &ClipRect,
    font_face: &tilesheet::Tilesheet,
    text: &str,
    mut x: i32,
    mut y: i32,
    width: i32,
    height: i32,
    fixed: bool,
    mapping: &[u8; TIC_PALETTE_SIZE],
    scale: i32,
    alt: bool,
) -> i32 {
    let mut pos = x;
    let mut max_x = x;

    for sym in text.bytes() {
        if sym == b'\n' {
            if pos > max_x { max_x = pos; }
            pos = x;
            y += height * scale;
        } else {
            let char_idx = if alt { TIC_FONT_CHARS } else { 0 } + sym as usize;
            let font_char = tilesheet::get_tile(font_face, char_idx as u32, true);
            let char_width = draw_char(ram, clip, &font_char, pos, y, scale, fixed, mapping);
            pos += (if !fixed && char_width > 0 { char_width + 1 } else { width }) * scale;
        }
    }

    if pos > max_x { pos - x } else { max_x - x }
}

pub fn tic_api_print(
    ram: &mut [u8],
    clip: &ClipRect,
    ram_font: *const u8,
    ram_tiles: *const u8,
    text: &str,
    mut x: i32,
    y: i32,
    color: u8,
    fixed: bool,
    scale: i32,
    alt: bool,
    blit_segment: u8,
) -> i32 {
    let mut mapping = [TRANSPARENT_COLOR; TIC_PALETTE_SIZE];
    mapping[1] = color;
    // Use segment 1 for font face
    let font_face = get_tilesheet_from_segment(ram_tiles, ram_font, 1);

    let font_width = if alt { 4 } else { 6 };
    let font_height = 6;
    let w = if !fixed { font_width - 2 } else { font_width };
    draw_text(ram, clip, &font_face, text, x, y, w, font_height, fixed, &mapping, scale, alt)
}

pub fn tic_api_font(
    ram: &mut [u8],
    clip: &ClipRect,
    ram_font: *const u8,
    ram_tiles: *const u8,
    text: &str,
    x: i32,
    y: i32,
    colors: &[u8],
    count: u8,
    w: i32,
    h: i32,
    fixed: bool,
    scale: i32,
    alt: bool,
    blit_segment: u8,
) -> i32 {
    let mapping = get_palette(&[0u8; 8], colors);

    let flipmask = {
        let mut s = (blit_segment >> 1) as u32;
        let mut mask = 1u8;
        while { s >>= 1; s != 0 } { mask <<= 1; }
        mask
    };

    let font_face = get_tilesheet_from_segment(ram_tiles, ram_font, blit_segment ^ flipmask);
    draw_text(ram, clip, &font_face, text, x, y, w, h, fixed, &mapping, scale, alt)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core;

    fn make_ram() -> Vec<u8> {
        vec![0u8; TIC80_WIDTH as usize * TIC80_HEIGHT as usize / 2]
    }

    fn default_clip() -> ClipRect {
        ClipRect { l: 0, t: 0, r: TIC80_WIDTH as i32, b: TIC80_HEIGHT as i32 }
    }

    #[test]
    fn set_and_get_pixel() {
        let mut ram = make_ram();
        let clip = default_clip();
        set_pixel(&mut ram, &clip, 10, 20, 0x5);
        assert_eq!(get_pixel(&ram, 10, 20), 0x5);
    }

    #[test]
    fn set_pixel_clipped() {
        let mut ram = make_ram();
        let clip = ClipRect { l: 5, t: 5, r: 10, b: 10 };
        set_pixel(&mut ram, &clip, 2, 2, 0x5); // outside clip
        assert_eq!(get_pixel(&ram, 2, 2), 0);
        set_pixel(&mut ram, &clip, 7, 7, 0xA); // inside clip
        assert_eq!(get_pixel(&ram, 7, 7), 0xA);
    }

    #[test]
    fn get_pixel_out_of_bounds() {
        let ram = make_ram();
        assert_eq!(get_pixel(&ram, -1, 0), 0);
        assert_eq!(get_pixel(&ram, 0, -1), 0);
        assert_eq!(get_pixel(&ram, TIC80_WIDTH as i32, 0), 0);
        assert_eq!(get_pixel(&ram, 0, TIC80_HEIGHT as i32), 0);
    }

    #[test]
    fn cls_fills_screen() {
        let mut ram = make_ram();
        let clip = default_clip();
        tic_api_cls(&mut ram, &clip, 0x5);
        // Check first and last pixel
        assert_eq!(get_pixel(&ram, 0, 0), 0x5);
        assert_eq!(get_pixel(&ram, TIC80_WIDTH as i32 - 1, TIC80_HEIGHT as i32 - 1), 0x5);
    }

    #[test]
    fn line_horizontal() {
        let mut ram = make_ram();
        let clip = default_clip();
        tic_api_line(&mut ram, &clip, 10.0, 5.0, 20.0, 5.0, 0x3);
        assert_eq!(get_pixel(&ram, 10, 5), 0x3);
        assert_eq!(get_pixel(&ram, 15, 5), 0x3);
        assert_eq!(get_pixel(&ram, 20, 5), 0x3);
    }

    #[test]
    fn rect_filled() {
        let mut ram = make_ram();
        let clip = default_clip();
        tic_api_rect(&mut ram, &clip, 5, 5, 10, 10, 0x7);
        assert_eq!(get_pixel(&ram, 5, 5), 0x7);
        assert_eq!(get_pixel(&ram, 14, 14), 0x7);
        assert_eq!(get_pixel(&ram, 4, 5), 0); // outside
    }

    #[test]
    fn pix_read() {
        let mut ram = make_ram();
        let clip = default_clip();
        tic_api_pix(&mut ram, &clip, 30, 40, 0x9, false);
        assert_eq!(tic_api_pix(&mut ram, &clip, 30, 40, 0, true), 0x9);
    }

    #[test]
    fn clip_rect() {
        let mut clip = default_clip();
        tic_api_clip(&mut clip, 10, 10, 50, 50);
        assert_eq!(clip.l, 10);
        assert_eq!(clip.t, 10);
        assert_eq!(clip.r, 60);
        assert_eq!(clip.b, 60);
    }

    #[test]
    fn flags() {
        let mut flags = [0u8; TIC_FLAGS];
        tic_api_fset(&mut flags, 5, 2, true);
        assert!(tic_api_fget(&flags, 5, 2));
        assert!(!tic_api_fget(&flags, 5, 1));
        tic_api_fset(&mut flags, 5, 2, false);
        assert!(!tic_api_fget(&flags, 5, 2));
    }

    #[test]
    fn circle_border() {
        let mut ram = make_ram();
        let clip = default_clip();
        tic_api_circb(&mut ram, &clip, 50, 50, 10, 0x4);
        // Check some points on the circle
        let has_any_pixels = (0..TIC80_HEIGHT as i32).any(|y|
            (0..TIC80_WIDTH as i32).any(|x| get_pixel(&ram, x, y) == 0x4)
        );
        assert!(has_any_pixels, "circb should draw some pixels");
    }

    #[test]
    fn ellipse_filled() {
        let mut ram = make_ram();
        let clip = default_clip();
        tic_api_elli(&mut ram, &clip, 100, 100, 20, 10, 0x6);
        // Center should be filled
        assert_eq!(get_pixel(&ram, 100, 100), 0x6);
    }

    #[test]
    fn paint_fill() {
        let mut ram = make_ram();
        let clip = default_clip();
        // Draw a border rect
        tic_api_rectb(&mut ram, &clip, 10, 10, 20, 20, 0x1);
        // Fill inside
        flood_fill(&mut ram, &clip, 15, 15, 0x2, 0x1);
        assert_eq!(get_pixel(&ram, 15, 15), 0x2);
        // Border should remain
        assert_eq!(get_pixel(&ram, 10, 10), 0x1);
    }

    #[test]
    fn print_text() {
        let mut ram = make_ram();
        let clip = default_clip();
        let mut font = [0u8; 4096];
        let result = tic_api_print(
            &mut ram, &clip,
            font.as_ptr(), font.as_ptr(),
            "A", 10, 10, 0xF, true, 1, false, 0,
        );
        assert!(result > 0);
    }
}
