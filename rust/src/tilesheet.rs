//! Tilesheet addressing — sprite/tile pixel access.
//!
//! Port of TIC-80's `src/tilesheet.c` + `src/tilesheet.h`.
//!
//! Maps logical (x, y) pixel coordinates and tile indices to physical
//! byte offsets in the sprite RAM, supporting 1/2/4 bits-per-pixel
//! formats with bank/page layout.

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SPRITESIZE: usize = 8;
const BITS_IN_BYTE: usize = 8;
const TIC_PALETTE_BPP: usize = 4;
/// sizeof(tic_tile) = 8 * 8 * 4 / 8 = 32
const TILE_SIZE: usize = SPRITESIZE * SPRITESIZE * TIC_PALETTE_BPP / BITS_IN_BYTE;
const SPRITESHEET_SIZE: u32 = 128;
const SPRITESHEET_COLS: u32 = SPRITESHEET_SIZE / SPRITESIZE as u32;
const BANK_SPRITES: u32 = 256;

// ---------------------------------------------------------------------------
// Peek/Poke helpers
// ---------------------------------------------------------------------------

/// Read 1 bit at bit-index `index` from byte buffer `addr`.
unsafe fn peek1(addr: *const u8, index: u32) -> u8 {
    (*addr.add((index >> 3) as usize) >> (index & 7)) & 1
}
/// Write 1 bit at bit-index `index` into byte buffer `addr`.
unsafe fn poke1(addr: *mut u8, index: u32, value: u8) {
    let p = addr.add((index >> 3) as usize);
    let shift = index & 7;
    *p = (*p & !(1 << shift)) | ((value & 1) << shift);
}
/// Read 2 bits at bit-index `index`.
unsafe fn peek2(addr: *const u8, index: u32) -> u8 {
    (*addr.add((index >> 2) as usize) >> ((index & 3) << 1)) & 3
}
/// Write 2 bits at bit-index `index`.
unsafe fn poke2(addr: *mut u8, index: u32, value: u8) {
    let p = addr.add((index >> 2) as usize);
    let shift = (index & 3) << 1;
    *p = (*p & !(3 << shift)) | ((value & 3) << shift);
}
/// Read 4 bits at bit-index `index`.
unsafe fn peek4(addr: *const u8, index: u32) -> u8 {
    (*addr.add((index >> 1) as usize) >> ((index & 1) << 2)) & 15
}
/// Write 4 bits at bit-index `index`.
unsafe fn poke4(addr: *mut u8, index: u32, value: u8) {
    let p = addr.add((index >> 1) as usize);
    let shift = (index & 1) << 2;
    *p = (*p & !(15 << shift)) | ((value & 15) << shift);
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Bit-depth mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Bpp {
    Bpp4 = 4,
    Bpp2 = 2,
    Bpp1 = 1,
}

/// Segment descriptor — maps one sprite bank/page layout.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct BlitSegment {
    pub page_orig: u32,
    pub bank_orig: u32,
    pub nb_pages: u32,
    pub bank_size: u32,
    pub sheet_width: u32,
    pub tile_width: u32,
    pub ptr_size: usize,
    peek: unsafe fn(*const u8, u32) -> u8,
    poke: unsafe fn(*mut u8, u32, u8),
}

/// A tilesheet referencing a segment and a byte pointer into sprite RAM.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Tilesheet<'a> {
    pub segment: &'a BlitSegment,
    pub ptr: *mut u8,
}

/// A single tile reference within a tilesheet.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TilePtr<'a> {
    pub segment: &'a BlitSegment,
    pub offset: u32,
    pub ptr: *mut u8,
}

/// Blit mode descriptor.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Blit {
    pub mode: Bpp,
    pub pages: u8,
    pub page: u8,
    pub bank: u8,
}

// ---------------------------------------------------------------------------
// Segment table
// ---------------------------------------------------------------------------

const SEGMENTS: &[BlitSegment] = &[
    //    page_orig bank_orig nb_pages bank_size sheet_w tile_w ptr_size  peek    poke
    // system gfx
    BlitSegment { page_orig: 0, bank_orig: 0, nb_pages: 1, bank_size: 256, sheet_width: 16, tile_width: 8, ptr_size: SPRITESIZE, peek: peek1, poke: poke1 },
    // system font
    BlitSegment { page_orig: 0, bank_orig: 0, nb_pages: 1, bank_size: 256, sheet_width: 16, tile_width: 8, ptr_size: SPRITESIZE, peek: peek1, poke: poke1 },
    // 4bpp p0 bg
    BlitSegment { page_orig: 0, bank_orig: 0, nb_pages: 1, bank_size: 256, sheet_width: 16, tile_width: 8, ptr_size: TILE_SIZE, peek: peek4, poke: poke4 },
    // 4bpp p0 fg
    BlitSegment { page_orig: 0, bank_orig: 1, nb_pages: 1, bank_size: 256, sheet_width: 16, tile_width: 8, ptr_size: TILE_SIZE, peek: peek4, poke: poke4 },
    // 2bpp p0 bg
    BlitSegment { page_orig: 0, bank_orig: 0, nb_pages: 2, bank_size: 512, sheet_width: 32, tile_width: 16, ptr_size: TILE_SIZE, peek: peek2, poke: poke2 },
    // 2bpp p1 bg
    BlitSegment { page_orig: 0, bank_orig: 1, nb_pages: 2, bank_size: 512, sheet_width: 32, tile_width: 16, ptr_size: TILE_SIZE, peek: peek2, poke: poke2 },
    // 2bpp p0 fg
    BlitSegment { page_orig: 0, bank_orig: 0, nb_pages: 2, bank_size: 512, sheet_width: 32, tile_width: 16, ptr_size: TILE_SIZE, peek: peek2, poke: poke2 },
    // 2bpp p1 fg
    BlitSegment { page_orig: 1, bank_orig: 0, nb_pages: 2, bank_size: 512, sheet_width: 32, tile_width: 16, ptr_size: TILE_SIZE, peek: peek2, poke: poke2 },
    // 1bpp p0 bg
    BlitSegment { page_orig: 0, bank_orig: 0, nb_pages: 4, bank_size: 1024, sheet_width: 64, tile_width: 32, ptr_size: TILE_SIZE, peek: peek1, poke: poke1 },
    // 1bpp p1 bg
    BlitSegment { page_orig: 1, bank_orig: 0, nb_pages: 4, bank_size: 1024, sheet_width: 64, tile_width: 32, ptr_size: TILE_SIZE, peek: peek1, poke: poke1 },
    // 1bpp p2 bg
    BlitSegment { page_orig: 2, bank_orig: 0, nb_pages: 4, bank_size: 1024, sheet_width: 64, tile_width: 32, ptr_size: TILE_SIZE, peek: peek1, poke: poke1 },
    // 1bpp p3 bg
    BlitSegment { page_orig: 3, bank_orig: 0, nb_pages: 4, bank_size: 1024, sheet_width: 64, tile_width: 32, ptr_size: TILE_SIZE, peek: peek1, poke: poke1 },
    // 1bpp p0 fg
    BlitSegment { page_orig: 0, bank_orig: 1, nb_pages: 4, bank_size: 1024, sheet_width: 64, tile_width: 32, ptr_size: TILE_SIZE, peek: peek1, poke: poke1 },
    // 1bpp p1 fg
    BlitSegment { page_orig: 1, bank_orig: 1, nb_pages: 4, bank_size: 1024, sheet_width: 64, tile_width: 32, ptr_size: TILE_SIZE, peek: peek1, poke: poke1 },
    // 1bpp p2 fg
    BlitSegment { page_orig: 2, bank_orig: 1, nb_pages: 4, bank_size: 1024, sheet_width: 64, tile_width: 32, ptr_size: TILE_SIZE, peek: peek1, poke: poke1 },
    // 1bpp p3 fg
    BlitSegment { page_orig: 3, bank_orig: 1, nb_pages: 4, bank_size: 1024, sheet_width: 64, tile_width: 32, ptr_size: TILE_SIZE, peek: peek1, poke: poke1 },
];

// ---------------------------------------------------------------------------
// Tilesheet functions
// ---------------------------------------------------------------------------

/// Create a tilesheet from segment index and raw RAM pointer.
pub fn get(segment_idx: u8, ptr: *mut u8) -> Tilesheet<'static> {
    assert!((segment_idx as usize) < SEGMENTS.len(), "segment index out of range");
    Tilesheet {
        segment: &SEGMENTS[segment_idx as usize],
        ptr,
    }
}

/// Get a tile pointer from a tilesheet by tile index.
pub fn get_tile<'a>(sheet: &'a Tilesheet<'a>, mut index: u32, local: bool) -> TilePtr<'a> {
    const COLS: u32 = 16;
    const SIZE: u32 = 8;
    let seg = sheet.segment;

    let (bank, page, iy, ix): (u32, u32, u32, u32) = if local {
        index &= 255;
        let ixy = (index / COLS, index % COLS);
        (seg.bank_orig, seg.page_orig, ixy.0, ixy.1)
    } else {
        let ia = (index / seg.bank_size, index % seg.bank_size);
        let ib = (ia.1 / seg.sheet_width, ia.1 % seg.sheet_width);
        let ic = (ib.1 / COLS, ib.1 % COLS);
        let bank = (ia.0 + seg.bank_orig) % 2;
        let page = (ic.0 + seg.page_orig) % seg.nb_pages;
        let iy = ib.0 % COLS;
        let ix = ic.1;
        (bank, page, iy, ix)
    };

    let xdiv = (ix / seg.nb_pages, ix % seg.nb_pages);
    let ptr_offset = (bank * COLS + iy) * COLS + page * COLS / seg.nb_pages + xdiv.0;
    let ptr = unsafe { sheet.ptr.add(seg.ptr_size * ptr_offset as usize) };
    let offset = xdiv.1 * SIZE;

    TilePtr {
        segment: seg,
        offset,
        ptr,
    }
}

/// Get pixel at (x, y) from a tilesheet.
pub fn get_pix(sheet: &Tilesheet, x: i32, y: i32) -> u8 {
    let seg = sheet.segment;
    let bank_offset = (((y >> 7) + seg.bank_orig as i32) & 1) << 8;
    let page_offset = ((((x >> 7) + seg.page_orig as i32) % seg.nb_pages as i32) << 4) / seg.nb_pages as i32;

    let tile_index = bank_offset + (((y & 127) >> 3) << 4) + page_offset + ((x & 127) / seg.tile_width as i32);
    let pix_addr = ((x & (seg.tile_width as i32 - 1)) + ((y & 7) * seg.tile_width as i32)) as u32;

    unsafe { (seg.peek)(sheet.ptr.add(tile_index as usize * seg.ptr_size), pix_addr) }
}

/// Set pixel at (x, y) in a tilesheet.
pub fn set_pix(sheet: &Tilesheet, x: i32, y: i32, value: u8) {
    let seg = sheet.segment;
    let bank_offset = (((y >> 7) + seg.bank_orig as i32) & 1) << 8;
    let page_offset = ((((x >> 7) + seg.page_orig as i32) % seg.nb_pages as i32) << 4) / seg.nb_pages as i32;

    let tile_index = bank_offset + (((y & 127) >> 3) << 4) + page_offset + ((x & 127) / seg.tile_width as i32);
    let pix_addr = ((x & (seg.tile_width as i32 - 1)) + ((y & 7) * seg.tile_width as i32)) as u32;

    unsafe { (seg.poke)(sheet.ptr.add(tile_index as usize * seg.ptr_size), pix_addr, value) }
}

/// Get pixel from a tile pointer at local (x, y).
pub fn get_tile_pix(tile: &TilePtr, x: i32, y: i32) -> u8 {
    let addr = tile.offset + x as u32 + (y as u32 * tile.segment.tile_width);
    unsafe { (tile.segment.peek)(tile.ptr, addr) }
}

/// Set pixel in a tile pointer at local (x, y).
pub fn set_tile_pix(tile: &TilePtr, x: i32, y: i32, value: u8) {
    let addr = tile.offset + x as u32 + (y as u32 * tile.segment.tile_width);
    unsafe { (tile.segment.poke)(tile.ptr, addr, value) }
}

// ---------------------------------------------------------------------------
// Blit helpers
// ---------------------------------------------------------------------------

pub fn blit_calc_segment(blit: &Blit) -> i32 {
    (blit.pages as i32) * (2 + blit.bank as i32) + blit.page as i32
}

pub fn blit_update_bpp(blit: &mut Blit, bpp: Bpp) {
    blit.mode = bpp;
    blit.pages = 4 / bpp as u8;
    blit.page %= blit.pages;
}

pub fn blit_calc_index(blit: &Blit) -> i32 {
    (blit.bank as i32) * (blit.pages as i32) * (BANK_SPRITES as i32)
        + (blit.page as i32) * (SPRITESHEET_COLS as i32)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Allocate a fake sprite RAM and test segment 0 (system gfx, 1bpp)
    #[test]
    fn create_tilesheet() {
        let mut ram = [0u8; 65536];
        let sheet = get(0, ram.as_mut_ptr());
        assert_eq!(sheet.segment.nb_pages, 1);
        assert_eq!(sheet.segment.bank_size, 256);
    }

    /// Write and read back a pixel via 4bpp segment
    #[test]
    fn write_read_pixel_4bpp() {
        let mut ram = [0u8; 65536];
        let sheet = get(2, ram.as_mut_ptr()); // segment 2 = 4bpp p0 bg

        set_pix(&sheet, 0, 0, 0x5);
        assert_eq!(get_pix(&sheet, 0, 0), 0x5);

        set_pix(&sheet, 1, 0, 0xA);
        assert_eq!(get_pix(&sheet, 1, 0), 0xA);
        assert_eq!(get_pix(&sheet, 0, 0), 0x5); // unchanged
    }

    /// Multiple pixels in different tiles
    #[test]
    fn write_read_multi_tile() {
        let mut ram = [0u8; 65536];
        let sheet = get(2, ram.as_mut_ptr()); // 4bpp p0 bg

        // Write at (0,0) and (20,0) (different tiles)
        set_pix(&sheet, 0, 0, 1);
        set_pix(&sheet, 20, 0, 2);
        assert_eq!(get_pix(&sheet, 0, 0), 1);
        assert_eq!(get_pix(&sheet, 20, 0), 2);
    }

    /// Tile pointer: get tile 0, read pixel at (0,0)
    #[test]
    fn tileptr_basic() {
        let mut ram = [0u8; 65536];
        let sheet = get(2, ram.as_mut_ptr()); // 4bpp p0 bg

        set_pix(&sheet, 0, 0, 0x7);
        let tile = get_tile(&sheet, 0, true);
        assert_eq!(get_tile_pix(&tile, 0, 0), 0x7);
    }

    /// Local (wrap) vs non-local (reindex) tile lookup
    #[test]
    fn local_vs_nonlocal() {
        let mut ram = [0u8; 65536];
        let sheet = get(2, ram.as_mut_ptr());

        // For local, index 0 and index 256 should give same tile (local wraps with &255)
        let tile_local_0 = get_tile(&sheet, 0, true);
        let tile_local_256 = get_tile(&sheet, 256, true);
        assert_eq!(tile_local_0.offset, tile_local_256.offset);
        assert_eq!(tile_local_0.ptr, tile_local_256.ptr);
    }

    /// Blit helpers
    #[test]
    fn blit_calc() {
        let mut b = Blit {
            mode: Bpp::Bpp4,
            pages: 1,
            page: 0,
            bank: 0,
        };
        assert_eq!(blit_calc_segment(&b), 2);

        blit_update_bpp(&mut b, Bpp::Bpp2);
        assert_eq!(b.pages, 2);
        assert_eq!(blit_calc_segment(&b), 4);
    }

    #[test]
    fn blit_calc_index_4bpp() {
        let b = Blit {
            mode: Bpp::Bpp4,
            pages: 1,
            page: 0,
            bank: 0,
        };
        // 0 * 1 * 256 + 0 * 16 = 0
        assert_eq!(blit_calc_index(&b), 0);
    }

    #[test]
    fn blit_calc_index_1bpp() {
        let b = Blit {
            mode: Bpp::Bpp1,
            pages: 4,
            page: 0,
            bank: 0,
        };
        // 0 * 4 * 256 + 0 * 16 = 0
        assert_eq!(blit_calc_index(&b), 0);
    }

    /// Peek/poke bit-level correctness
    #[test]
    fn peek_poke_4() {
        let mut buf = [0u8; 4];
        let ptr = buf.as_mut_ptr();
        unsafe {
            poke4(ptr, 0, 0x5); // nibble 0 = 5
            poke4(ptr, 1, 0xA); // nibble 1 = A
        }
        assert_eq!(buf[0], 0xA5); // A in high nibble, 5 in low
        unsafe {
            assert_eq!(peek4(ptr, 0), 0x5);
            assert_eq!(peek4(ptr, 1), 0xA);
        }
    }

    #[test]
    fn peek_poke_1() {
        let mut buf = [0u8; 2];
        let ptr = buf.as_mut_ptr();
        unsafe {
            poke1(ptr, 0, 1);
            poke1(ptr, 3, 1);
        }
        assert_eq!(buf[0], 0b00001001);
        unsafe {
            assert_eq!(peek1(ptr, 0), 1);
            assert_eq!(peek1(ptr, 1), 0);
            assert_eq!(peek1(ptr, 3), 1);
        }
    }

    #[test]
    fn peek_poke_2() {
        let mut buf = [0u8; 2];
        let ptr = buf.as_mut_ptr();
        unsafe {
            poke2(ptr, 0, 1);
            poke2(ptr, 1, 3);
        }
        // byte 0: bits [1..3) = 01, bits [3..5) = 11 => 0b1101 = 0xD
        // Wait: poke2 with index 0: shift = (0&3)<<1 = 0, val = 1 => bits 0-1 = 01
        // poke2 with index 1: shift = (1&3)<<1 = 2, val = 3 => bits 2-3 = 11
        // So byte 0 = 0b00001101 = 0x0D
        assert_eq!(buf[0], 0b00001101);
    }

    /// All 16 segments can be accessed
    #[test]
    fn all_segments() {
        let mut ram = [0u8; 65536];
        for i in 0..16u8 {
            let sheet = get(i, ram.as_mut_ptr());
            assert_eq!(sheet.segment.nb_pages, SEGMENTS[i as usize].nb_pages);
        }
    }

    /// Pixel coordinates outside bounds should not crash (wrap behavior)
    #[test]
    fn edge_coordinates() {
        let mut ram = [0u8; 65536];
        let sheet = get(2, ram.as_mut_ptr());
        // These should not panic (C behavior: uses & 127 etc.)
        set_pix(&sheet, 200, 200, 0xF);
        let v = get_pix(&sheet, 200, 200);
        assert_eq!(v, 0xF);
    }
}
