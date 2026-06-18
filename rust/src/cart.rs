//! Cartridge load/save — TIC-80 `.tic` chunk format.
//!
//! Port of TIC-80's `src/cart.c`.
//!
//! A `.tic` cartridge is a sequence of 4-byte chunk headers followed by
//! chunk data.  Each chunk header encodes type (5 bits), bank (3 bits),
//! size (16 bits LE) and a temp byte.

// unused in current port; kept for backward compat

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TIC_BANK_BITS: u32 = 3;
const TIC_BANKS: usize = 1 << TIC_BANK_BITS; // 8
const TIC_BANKSIZE_BITS: u32 = 16;
const TIC_BANK_SIZE: usize = 1 << TIC_BANKSIZE_BITS; // 64K
const TIC_CODE_SIZE: usize = TIC_BANK_SIZE * TIC_BANKS; // 512K
const TIC_BINARY_BANKS: usize = 4;
const TIC_BINARY_SIZE: usize = TIC_BINARY_BANKS * TIC_BANK_SIZE; // 256K
const TIC80_WIDTH: usize = 240;
const TIC80_HEIGHT: usize = 136;
const TIC_PALETTE_SIZE: usize = 16;
const TIC_PALETTE_BPP: usize = 4;
const MUSIC_PATTERNS: usize = 60;
const MUSIC_PATTERN_ROWS: usize = 64;
const MAX_VOLUME: u8 = 15;

// ---------------------------------------------------------------------------
// Chunk types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ChunkType {
    Dummy = 0,
    Tiles = 1,
    Sprites = 2,
    CoverDep = 3,
    Map = 4,
    Code = 5,
    Flags = 6,
    Temp2 = 7,
    Temp3 = 8,
    Samples = 9,
    Waveform = 10,
    Temp4 = 11,
    Palette = 12,
    PatternsDep = 13,
    Music = 14,
    Patterns = 15,
    CodeZip = 16,
    Default = 17,
    Screen = 18,
    Binary = 19,
    Lang = 20,
}

// ---------------------------------------------------------------------------
// Chunk header packing (always little-endian on disk)
//
// Bit layout (LE CPU):
//   bits 0..4  : type (5 bits)
//   bits 5..7  : bank (3 bits)
//   bits 8..23 : size (16 bits)
//   bits 24..31: temp (8 bits)
// ---------------------------------------------------------------------------

#[repr(C, packed)]
struct ChunkHeader {
    raw: u32,
}

impl ChunkHeader {
    fn read(ptr: &[u8]) -> Self {
        let mut raw = [0u8; 4];
        raw.copy_from_slice(&ptr[..4]);
        ChunkHeader {
            raw: u32::from_le_bytes(raw),
        }
    }

    fn typ(&self) -> ChunkType {
        match self.raw & 0x1f {
            0 => ChunkType::Dummy,
            1 => ChunkType::Tiles,
            2 => ChunkType::Sprites,
            3 => ChunkType::CoverDep,
            4 => ChunkType::Map,
            5 => ChunkType::Code,
            6 => ChunkType::Flags,
            7 => ChunkType::Temp2,
            8 => ChunkType::Temp3,
            9 => ChunkType::Samples,
            10 => ChunkType::Waveform,
            11 => ChunkType::Temp4,
            12 => ChunkType::Palette,
            13 => ChunkType::PatternsDep,
            14 => ChunkType::Music,
            15 => ChunkType::Patterns,
            16 => ChunkType::CodeZip,
            17 => ChunkType::Default,
            18 => ChunkType::Screen,
            19 => ChunkType::Binary,
            20 => ChunkType::Lang,
            _ => ChunkType::Dummy,
        }
    }

    fn bank(&self) -> usize {
        ((self.raw >> 5) & 0x7) as usize
    }

    fn size_raw(&self) -> u16 {
        ((self.raw >> 8) & 0xffff) as u16
    }

    fn chunk_size(&self) -> usize {
        let s = self.size_raw() as usize;
        if s == 0 && (self.typ() == ChunkType::Code || self.typ() == ChunkType::Binary) {
            TIC_BANK_SIZE
        } else {
            s
        }
    }

    fn write(buf: &mut [u8], typ: u32, bank: u32, size: u16) {
        let raw = typ | (bank << 5) | ((size as u32) << 8);
        buf[..4].copy_from_slice(&raw.to_le_bytes());
    }
}

// ---------------------------------------------------------------------------
// Cartridge structures (minimal — mirrors C types)
// ---------------------------------------------------------------------------

/// RGB color.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Palette — 16 RGB colors.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Palette {
    pub colors: [Rgb; TIC_PALETTE_SIZE],
}

/// Waveform data.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Waveforms {
    pub data: [u8; 68 * 12], // approximate, exact size from C
}

/// Samples data (SFX).
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Samples {
    pub data: [u8; 64 * 68], // approximate
}

/// Track row (as raw bytes for bitfield compatibility).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TrackRow(pub [u8; 3]);

/// Track pattern.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Pattern {
    pub rows: [TrackRow; MUSIC_PATTERN_ROWS],
}

/// Patterns collection.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Patterns {
    pub data: [Pattern; MUSIC_PATTERNS],
}

/// Track data.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Tracks {
    pub data: [u8; 16 * 3 * 8], // approximate
}

/// Map data.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Map {
    pub data: [u8; 128 * 128 / 2], // approximate
}

/// Screen buffer.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Screen {
    pub data: [u8; TIC80_WIDTH * TIC80_HEIGHT * TIC_PALETTE_BPP / 8],
}

impl Default for Screen {
    fn default() -> Self {
        Screen {
            data: [0u8; TIC80_WIDTH * TIC80_HEIGHT * TIC_PALETTE_BPP / 8],
        }
    }
}

/// One bank of cartridge data.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Bank {
    pub screen: Screen,
    pub tiles: [u8; 256 * 32],   // approximate
    pub sprites: [u8; 256 * 32], // approximate
    pub map: [u8; 16384],        // approximate
    pub sfx: [u8; 16384],        // approximate
    pub music: [u8; 16384],      // approximate
    pub flags: [u8; 256],
    pub palette: Vec<u8>,
}

impl Default for Bank {
    fn default() -> Self {
        Bank {
            screen: Screen::default(),
            tiles: [0u8; 256 * 32],
            sprites: [0u8; 256 * 32],
            map: [0u8; 16384],
            sfx: [0u8; 16384],
            music: [0u8; 16384],
            flags: [0u8; 256],
            palette: Vec::new(),
        }
    }
}

/// A loaded cartridge.
#[derive(Clone, Debug)]
pub struct Cartridge {
    pub banks: [Vec<u8>; TIC_BANKS],
    pub code: Vec<u8>,
    pub binary: Vec<u8>,
    pub binary_size: u32,
    pub lang: u8,
}

impl Default for Cartridge {
    fn default() -> Self {
        Cartridge {
            banks: Default::default(),
            code: Vec::new(),
            binary: Vec::new(),
            binary_size: 0,
            lang: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Default data
// ---------------------------------------------------------------------------

/// Sweetie16 palette data (48 bytes = 16× RGB).
const SWEETIE16: [u8; 48] = [
    0x1a, 0x1c, 0x2c, 0x5d, 0x27, 0x5d, 0xb1, 0x3e, 0x53, 0xef, 0x7d, 0x57,
    0xff, 0xcd, 0x75, 0xa7, 0xf0, 0x70, 0x38, 0xb7, 0x64, 0x25, 0x71, 0x79,
    0x29, 0x36, 0x6f, 0x3b, 0x5d, 0xc9, 0x41, 0xa6, 0xf6, 0x73, 0xef, 0xf7,
    0xf4, 0xf4, 0xf4, 0x94, 0xb0, 0xc2, 0x56, 0x6c, 0x86, 0x33, 0x3c, 0x57,
];

/// Default waveforms (48 bytes).
const WAVEFORMS: [u8; 48] = [
    0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
    0xff, 0xff, 0xff, 0xff, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe,
    0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01, 0x10, 0x32, 0x54, 0x76,
    0x98, 0xba, 0xdc, 0xfe, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe,
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Trim trailing zero bytes from a buffer.
fn calc_buffer_size(buf: &[u8]) -> usize {
    buf.iter()
        .rposition(|&b| b != 0)
        .map_or(0, |i| i + 1)
}

/// Copy at most `max_len` bytes from `src` to `dst`, return bytes copied.
fn load_chunk_data(dst: &mut [u8], src: &[u8], chunk_size: u16) -> usize {
    let sz = if chunk_size == 0 {
        TIC_BANK_SIZE
    } else {
        chunk_size as usize
    };
    let copy_len = sz.min(dst.len());
    dst[..copy_len].copy_from_slice(&src[..copy_len]);
    copy_len
}

// ---------------------------------------------------------------------------
// Cartridge load
// ---------------------------------------------------------------------------

/// Load a cartridge from raw chunk data.
///
/// `data` must be a sequence of ChunkHeader + payload.
/// Returns `None` if the data appears to be in PNG format (starts with
/// `\x89PNG`) — use the PNG decoder before calling this.
pub fn cart_load(cart: &mut Cartridge, data: &[u8]) {
    // Check for PNG wrapper
    if data.starts_with(b"\x89PNG") {
        // PNG not yet implemented in this port — return empty cartridge
        return;
    }

    let end = data.len();
    let mut ptr: usize = 0;

    // --- First pass: load palette chunks + defaults ---
    while ptr + 4 <= end {
        let header = ChunkHeader::read(&data[ptr..]);
        ptr += 4;
        let chunk_sz = header.chunk_size();
        let chunk_end = (ptr + chunk_sz).min(end);

        match header.typ() {
            ChunkType::Palette => {
                let bank = header.bank();
                if bank < TIC_BANKS {
                    let src = &data[ptr..chunk_end];
                    cart.banks[bank].clear();
                    cart.banks[bank].extend_from_slice(src);
                }
            }
            ChunkType::Default => {
                let bank = header.bank();
                if bank < TIC_BANKS {
                    cart.banks[bank].clear();
                    cart.banks[bank].extend_from_slice(&SWEETIE16);
                    cart.banks[bank].extend_from_slice(&WAVEFORMS);
                }
            }
            _ => {}
        }
        ptr = chunk_end;
    }

    // Fallback: if bank 0 has no palette, use DB16 (matching C behavior)
    if cart.banks[0].is_empty() {
        const DB16: [u8; 48] = [
            0x14, 0x0c, 0x1c, 0x44, 0x24, 0x34, 0x30, 0x34, 0x6d, 0x4e, 0x4a,
            0x4e, 0x85, 0x4c, 0x30, 0x34, 0x65, 0x24, 0xd0, 0x46, 0x48, 0x75,
            0x71, 0x61, 0x59, 0x7d, 0xce, 0xd2, 0x7d, 0x2c, 0x85, 0x95, 0xa1,
            0x6d, 0xaa, 0x2c, 0xd2, 0xaa, 0x99, 0x6d, 0xc2, 0xca, 0xda, 0xd4,
            0x5e, 0xde, 0xee, 0xd6,
        ];
        cart.banks[0].extend_from_slice(&DB16);
    }

    // --- Second pass: load all other chunks ---
    struct CodeRef<'a> {
        size: usize,
        data: &'a [u8],
    }
    struct BinaryRef<'a> {
        size: usize,
        data: &'a [u8],
    }

    let mut code_chunks: [Option<CodeRef>; TIC_BANKS] = Default::default();
    let mut binary_chunks: [Option<BinaryRef>; TIC_BINARY_BANKS] = Default::default();

    ptr = 0;
    while ptr + 4 <= end {
        let header = ChunkHeader::read(&data[ptr..]);
        ptr += 4;
        let chunk_sz = header.chunk_size();
        let chunk_end = (ptr + chunk_sz).min(end);

        match header.typ() {
            ChunkType::Code => {
                let bank = header.bank();
                if bank < TIC_BANKS {
                    code_chunks[bank] = Some(CodeRef {
                        size: chunk_sz,
                        data: &data[ptr..chunk_end],
                    });
                }
            }
            ChunkType::Binary => {
                let bank = header.bank();
                if bank < TIC_BINARY_BANKS {
                    binary_chunks[bank] = Some(BinaryRef {
                        size: chunk_sz,
                        data: &data[ptr..chunk_end],
                    });
                }
            }
            ChunkType::Lang => {
                if !data[ptr..chunk_end].is_empty() {
                    cart.lang = data[ptr]; // single byte
                }
            }
            _ => {}
        }
        ptr = chunk_end;
    }

    // Merge binary chunks (reverse order, like C's RFOR)
    cart.binary.clear();
    for chunk in binary_chunks.iter().rev() {
        if let Some(c) = chunk {
            cart.binary.extend_from_slice(&c.data[..c.size]);
            cart.binary_size += c.size as u32;
        }
    }

    // Merge code chunks (reverse order)
    cart.code.clear();
    if cart.code.is_empty() {
        for chunk in code_chunks.iter().rev() {
            if let Some(c) = chunk {
                cart.code.extend_from_slice(&c.data[..c.size]);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cartridge save
// ---------------------------------------------------------------------------

/// Save a cartridge into a byte buffer.
///
/// Returns the number of bytes written.
pub fn cart_save(cart: &Cartridge, buffer: &mut [u8]) -> usize {
    let _start_len = buffer.len();
    let mut pos = 0usize;

    let write_chunk =
        |buf: &mut [u8], pos: &mut usize, typ: u32, bank: u32, data: &[u8], fixed: bool| {
            let size = if fixed {
                data.len()
            } else {
                calc_buffer_size(data)
            };
            if size == 0 {
                return;
            }
            // Chunk header (4 bytes)
            ChunkHeader::write(&mut buf[*pos..], typ, bank, size as u16);
            *pos += 4;
            // Chunk data
            buf[*pos..*pos + size].copy_from_slice(&data[..size]);
            *pos += size;
        };

    // Default palettes for comparison
    let default_palette = &SWEETIE16;
    let _default_waveforms = &WAVEFORMS;

    for bank_idx in 0..TIC_BANKS {
        let bank_data = &cart.banks[bank_idx];

        // Check if palette + waveforms match defaults
        let _has_default_pal = bank_data.len() >= 48
            && bank_data[..48.min(bank_data.len())] == default_palette[..];

        // Simplified: always save palette chunk (or default marker)
        // Full optimization would check waveforms too
        write_chunk(buffer, &mut pos, ChunkType::Palette as u32, bank_idx as u32, bank_data, true);
    }

    // Save code
    if !cart.code.is_empty() {
        let code = &cart.code;
        let code_str = if let Ok(s) = std::str::from_utf8(code) {
            s.trim_end_matches('\0')
        } else {
            ""
        };
        let code_len = code_str.len();
        let num_chunks = code_len / TIC_BANK_SIZE;

        for i in (0..=num_chunks).rev() {
            let start = i * TIC_BANK_SIZE;
            let end = (start + TIC_BANK_SIZE).min(code_len);
            if start < code_len {
                write_chunk(
                    buffer,
                    &mut pos,
                    ChunkType::Code as u32,
                    i as u32,
                    &code[start..end],
                    true,
                );
            }
        }
    }

    // Save binary
    if cart.binary_size > 0 {
        let binary = &cart.binary[..cart.binary_size as usize];
        let num_chunks = binary.len() / TIC_BANK_SIZE;

        for i in (0..=num_chunks).rev() {
            let start = i * TIC_BANK_SIZE;
            let end = (start + TIC_BANK_SIZE).min(binary.len());
            if start < binary.len() {
                write_chunk(
                    buffer,
                    &mut pos,
                    ChunkType::Binary as u32,
                    i as u32,
                    &binary[start..end],
                    true,
                );
            }
        }
    }

    // Save language
    if cart.lang != 0 {
        write_chunk(buffer, &mut pos, ChunkType::Lang as u32, 0, &[cart.lang], true);
    }

    pos
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: save minimal cartridge → load → verify
    #[test]
    fn save_load_roundtrip() {
        let mut cart = Cartridge::default();

        // Add some palette data
        cart.banks[0].extend_from_slice(&SWEETIE16);

        // Add some code
        cart.code = b"print('hello')".to_vec();

        // Save
        let mut buf = vec![0u8; 1024 * 1024];
        let saved = cart_save(&cart, &mut buf);
        assert!(saved > 0, "save produced no output");

        // Load back
        let mut loaded = Cartridge::default();
        cart_load(&mut loaded, &buf[..saved]);

        // Verify
        assert_eq!(loaded.banks[0], cart.banks[0], "bank 0 palette mismatch");
        assert_eq!(loaded.code, cart.code, "code mismatch");
    }

    #[test]
    fn png_header_returns_empty() {
        let mut cart = Cartridge::default();
        let png_header = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        cart_load(&mut cart, &png_header);
        // Should not crash, banks should be empty (PNG not implemented)
        assert!(cart.banks[0].is_empty());
    }
}
