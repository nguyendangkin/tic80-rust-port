//! Core engine — RAM peek/poke, memcpy/memset, sync constants.
//!
//! Port of TIC-80's `src/core/core.c`.

use std::ptr;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const BITS_IN_BYTE: usize = 8;
pub const TIC80_WIDTH: u32 = 240;
pub const TIC80_HEIGHT: u32 = 136;
pub const TIC_BANKS: usize = 8;
pub const TIC_BANK_SIZE: usize = 65536;
pub const TIC_RAM_SIZE: usize = 256 * 1024;
pub const TIC_PALETTE_SIZE: usize = 16;
pub const TIC80_FRAMERATE: u32 = 60;

// ---------------------------------------------------------------------------
// Bit-level peek/poke helpers (private, raw pointer versions)
// ---------------------------------------------------------------------------

unsafe fn peek1_raw(addr: *const u8, index: u32) -> u8 {
    (*addr.add((index >> 3) as usize) >> (index & 7)) & 1
}
unsafe fn poke1_raw(addr: *mut u8, index: u32, value: u8) {
    let p = addr.add((index >> 3) as usize);
    let shift = index & 7;
    *p = (*p & !(1u8 << shift)) | ((value & 1) << shift);
}
unsafe fn peek2_raw(addr: *const u8, index: u32) -> u8 {
    (*addr.add((index >> 2) as usize) >> ((index & 3) << 1)) & 3
}
unsafe fn poke2_raw(addr: *mut u8, index: u32, value: u8) {
    let p = addr.add((index >> 2) as usize);
    let shift = (index & 3) << 1;
    *p = (*p & !(3u8 << shift)) | ((value & 3) << shift);
}
unsafe fn peek4_raw(addr: *const u8, index: u32) -> u8 {
    (*addr.add((index >> 1) as usize) >> ((index & 1) << 2)) & 15
}
unsafe fn poke4_raw(addr: *mut u8, index: u32, value: u8) {
    let p = addr.add((index >> 1) as usize);
    let shift = (index & 1) << 2;
    *p = (*p & !(15u8 << shift)) | ((value & 15) << shift);
}

// ---------------------------------------------------------------------------
// Public peek/poke API (slice-based)
// ---------------------------------------------------------------------------

const RAM_BITS: usize = TIC_RAM_SIZE * BITS_IN_BYTE;

pub fn peek(ram: &[u8], address: i32, bits: i32) -> u8 {
    if address < 0 {
        return 0;
    }
    let addr = address as usize;
    let ram_len = ram.len();
    match bits {
        1 if addr < RAM_BITS / 1 && addr / 8 < ram_len => unsafe { peek1_raw(ram.as_ptr(), addr as u32) },
        2 if addr < RAM_BITS / 2 && addr / 4 < ram_len => unsafe { peek2_raw(ram.as_ptr(), addr as u32) },
        4 if addr < RAM_BITS / 4 && addr / 2 < ram_len => unsafe { peek4_raw(ram.as_ptr(), addr as u32) },
        8 if addr < ram_len => ram[addr],
        _ => 0,
    }
}

pub fn poke(ram: &mut [u8], address: i32, value: u8, bits: i32) {
    if address < 0 {
        return;
    }
    let addr = address as usize;
    let ram_len = ram.len();
    match bits {
        1 if addr < RAM_BITS / 1 && addr / 8 < ram_len => unsafe { poke1_raw(ram.as_mut_ptr(), addr as u32, value) },
        2 if addr < RAM_BITS / 2 && addr / 4 < ram_len => unsafe { poke2_raw(ram.as_mut_ptr(), addr as u32, value) },
        4 if addr < RAM_BITS / 4 && addr / 2 < ram_len => unsafe { poke4_raw(ram.as_mut_ptr(), addr as u32, value) },
        8 if addr < ram_len => ram[addr] = value,
        _ => {}
    }
}

pub fn peek4(ram: &[u8], address: i32) -> u8 { peek(ram, address, 4) }
pub fn peek2(ram: &[u8], address: i32) -> u8 { peek(ram, address, 2) }
pub fn peek1(ram: &[u8], address: i32) -> u8 { peek(ram, address, 1) }
pub fn poke4(ram: &mut [u8], address: i32, value: u8) { poke(ram, address, value, 4) }
pub fn poke2(ram: &mut [u8], address: i32, value: u8) { poke(ram, address, value, 2) }
pub fn poke1(ram: &mut [u8], address: i32, value: u8) { poke(ram, address, value, 1) }

// ---------------------------------------------------------------------------
// Memcpy / Memset
// ---------------------------------------------------------------------------

pub fn memcpy_ram(ram: &mut [u8], dst: i32, src: i32, size: i32) {
    if size < 0 || size as usize > ram.len() {
        return;
    }
    let size = size as usize;
    let d = dst as usize;
    let s = src as usize;
    let bound = ram.len().saturating_sub(size);
    if d <= bound && s <= bound {
        let raw = ram.as_mut_ptr();
        unsafe {
            ptr::copy(raw.add(s), raw.add(d), size);
        }
    }
}

pub fn memset_ram(ram: &mut [u8], dst: i32, val: u8, size: i32) {
    if size < 0 || size as usize > ram.len() {
        return;
    }
    let size = size as usize;
    let d = dst as usize;
    let bound = ram.len().saturating_sub(size);
    if d <= bound {
        ram[d..d + size].fill(val);
    }
}

// ---------------------------------------------------------------------------
// Sync section masks (mirrors TIC_SYNC_LIST from api.h)
// ---------------------------------------------------------------------------

pub const SYNC_TILES: u32   = 1 << 0;
pub const SYNC_SPRITES: u32  = 1 << 1;
pub const SYNC_MAP: u32     = 1 << 2;
pub const SYNC_SFX: u32     = 1 << 3;
pub const SYNC_MUSIC: u32   = 1 << 4;
pub const SYNC_PALETTE: u32  = 1 << 5;
pub const SYNC_FLAGS: u32   = 1 << 6;
pub const SYNC_SCREEN: u32  = 1 << 7;
pub const SYNC_ALL: u32     = (1 << 8) - 1;
pub const SYNC_NO_SCREEN: u32 = SYNC_ALL & !SYNC_SCREEN;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peek8_basic() {
        let ram = vec![0xABu8; 256];
        assert_eq!(peek(&ram, 0, 8), 0xAB);
        assert_eq!(peek(&ram, 100, 8), 0xAB);
    }

    #[test]
    fn peek8_negative_address() {
        let ram = vec![0u8; 64];
        assert_eq!(peek(&ram, -1, 8), 0);
    }

    #[test]
    fn poke8_readback() {
        let mut ram = vec![0u8; 64];
        poke(&mut ram, 42, 0x77, 8);
        assert_eq!(ram[42], 0x77);
    }

    #[test]
    fn peek4_half_byte() {
        let mut ram = vec![0u8; 64];
        ram[0] = 0xAB;
        assert_eq!(peek(&ram, 0, 4), 0x0B);
        assert_eq!(peek(&ram, 1, 4), 0x0A);
    }

    #[test]
    fn poke4_then_peek4() {
        let mut ram = vec![0u8; 64];
        poke(&mut ram, 0, 0x5, 4);
        assert_eq!(peek(&ram, 0, 4), 0x05);
        poke(&mut ram, 1, 0xA, 4);
        assert_eq!(peek(&ram, 1, 4), 0x0A);
        assert_eq!(peek(&ram, 0, 4), 0x05);
    }

    #[test]
    fn peek_out_of_bounds() {
        let ram = vec![0u8; 64];
        assert_eq!(peek(&ram, 99999, 8), 0);
    }

    #[test]
    fn memcpy_within_bounds() {
        let mut ram = vec![0u8; 100];
        ram[5..10].copy_from_slice(&[1, 2, 3, 4, 5]);
        memcpy_ram(&mut ram, 20, 5, 5);
        assert_eq!(&ram[20..25], &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn memcpy_overlap_forward() {
        let mut ram = vec![0u8; 100];
        for i in 10..20 { ram[i] = (i - 9) as u8; }
        memcpy_ram(&mut ram, 15, 10, 5);
        assert_eq!(ram[15], 1);
        assert_eq!(ram[16], 2);
    }

    #[test]
    fn memcpy_oob_returns() {
        let mut ram = vec![0u8; 10];
        memcpy_ram(&mut ram, 8, 0, 5);
        assert_eq!(ram, vec![0u8; 10]);
    }

    #[test]
    fn memcpy_negative() {
        let mut ram = vec![0u8; 10];
        memcpy_ram(&mut ram, -1, 0, 5);
        assert_eq!(ram, vec![0u8; 10]);
    }

    #[test]
    fn memset_basic() {
        let mut ram = vec![0u8; 100];
        memset_ram(&mut ram, 20, 0xFF, 10);
        assert_eq!(&ram[20..30], &[0xFF; 10]);
        assert_eq!(ram[19], 0);
        assert_eq!(ram[30], 0);
    }

    #[test]
    fn memset_oob_returns() {
        let mut ram = vec![0u8; 10];
        memset_ram(&mut ram, 5, 0xFF, 10);
        assert_eq!(ram, vec![0u8; 10]);
    }

    #[test]
    fn sync_constants() {
        assert_eq!(SYNC_TILES, 1);
        assert_eq!(SYNC_SPRITES, 2);
        assert_eq!(SYNC_ALL, 0xFF);
        assert_eq!(SYNC_NO_SCREEN, 0x7F);
    }

    #[test]
    fn peek1_bit() {
        let mut ram = vec![0u8; 4];
        ram[0] = 0b00001000;
        assert_eq!(peek(&ram, 3, 1), 1);
        assert_eq!(peek(&ram, 0, 1), 0);
    }

    #[test]
    fn poke1_bit() {
        let mut ram = vec![0u8; 4];
        poke(&mut ram, 3, 1, 1);
        assert_eq!(ram[0], 0b00001000);
        poke(&mut ram, 3, 0, 1);
        assert_eq!(ram[0], 0);
    }

    #[test]
    fn peek2_bits() {
        let mut ram = vec![0u8; 4];
        ram[0] = 0b01100100;
        assert_eq!(peek(&ram, 0, 2), 0);
        assert_eq!(peek(&ram, 1, 2), 1);
        assert_eq!(peek(&ram, 2, 2), 2);
        assert_eq!(peek(&ram, 3, 2), 1);
    }

    #[test]
    fn peek4_convenience() {
        let mut ram = vec![0u8; 64];
        ram[0] = 0xAB;
        assert_eq!(peek4(&ram, 0), 0x0B);
        assert_eq!(peek4(&ram, 1), 0x0A);
    }

    #[test]
    fn poke4_convenience() {
        let mut ram = vec![0u8; 64];
        poke4(&mut ram, 0, 0x7);
        assert_eq!(ram[0], 0x07);
        poke4(&mut ram, 1, 0x8);
        assert_eq!(ram[0], 0x87);
    }

    #[test]
    fn memcpy_zero() {
        let mut ram = vec![0u8; 10];
        memcpy_ram(&mut ram, 0, 5, 0);
        assert_eq!(ram, vec![0u8; 10]);
    }

    #[test]
    fn memset_zero() {
        let mut ram = vec![0u8; 10];
        memset_ram(&mut ram, 0, 0xFF, 0);
        assert_eq!(ram, vec![0u8; 10]);
    }

    #[test]
    fn different_bit_sizes() {
        let mut ram = vec![0u8; 64];
        poke(&mut ram, 0, 0xA, 4);
        poke(&mut ram, 2, 0x3, 2);
        poke(&mut ram, 5, 1, 1);
        assert_eq!(peek(&ram, 0, 4), 0xA);
        assert_eq!(peek(&ram, 2, 2), 0x3);
        assert_eq!(peek(&ram, 5, 1), 1);
    }
}
