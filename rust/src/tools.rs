//! Utility functions — palette, pattern, string, buffer helpers.
//!
//! Port of TIC-80's `src/tools.c` + `src/tools.h`.

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BITS_IN_BYTE: usize = 8;
const TIC_PALETTE_SIZE: usize = 16;

// Track / pattern
const TIC_SOUND_CHANNELS: u32 = 4;
const TRACK_PATTERN_BITS: u32 = 6;
const TRACK_PATTERN_MASK: u32 = (1 << TRACK_PATTERN_BITS) - 1; // 63
const TRACK_PATTERNS_SIZE: u32 =
    TRACK_PATTERN_BITS * TIC_SOUND_CHANNELS / BITS_IN_BYTE as u32; // 3
const MUSIC_FRAMES: u32 = 16;

// SFX
const MUSIC_SFXID_LOW_BITS: u32 = 5;
const SFX_COUNT_BITS: u32 = 6;
pub const SFX_COUNT: u32 = 1 << SFX_COUNT_BITS; // 64

// Waveform
const WAVE_SIZE: usize = 68;

// ---------------------------------------------------------------------------
// Types (minimal, matching C layouts)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Palette {
    pub colors: [Rgb; TIC_PALETTE_SIZE],
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct BlitPal {
    pub data: [u32; TIC_PALETTE_SIZE],
}

/// Track row — 3 bytes matching C bitfield layout (little-endian).
///
/// Byte 0: note[3:0] | param1[7:4]
/// Byte 1: param2[3:0] | command[6:4] | sfxhi[7]
/// Byte 2: sfxlow[4:0] | octave[7:5]
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TrackRow(pub [u8; 3]);

impl TrackRow {
    pub fn sfx(&self) -> u32 {
        let sfxhi = ((self.0[1] >> 7) & 1) as u32;
        let sfxlow = (self.0[2] & 0x1f) as u32;
        (sfxhi << MUSIC_SFXID_LOW_BITS) | sfxlow
    }

    pub fn set_sfx(&mut self, mut sfx: u32) {
        if sfx >= SFX_COUNT {
            sfx = SFX_COUNT - 1;
        }
        self.0[1] = (self.0[1] & 0x7f) | (((sfx >> MUSIC_SFXID_LOW_BITS) & 1) as u8) << 7;
        self.0[2] = (self.0[2] & 0xe0) | (sfx & 0x1f) as u8;
    }
}

/// Track data.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Track {
    pub data: [u8; MUSIC_FRAMES as usize * TRACK_PATTERNS_SIZE as usize],
    pub tempo: i8,
    pub rows: u8,
    pub speed: i8,
}

/// Waveform.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Waveform {
    pub data: [u8; WAVE_SIZE],
}

// ---------------------------------------------------------------------------
// Pixel format enum
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PixelFormat {
    Bgra8888 = (4 << 8) | 32,
    Rgba8888 = (4 << 8) | 33,
    Abgr8888 = (4 << 8) | 34,
    Argb8888 = (4 << 8) | 35,
}

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

/// SFX position from speed and ticks.
pub fn sfx_pos(speed: i32, ticks: i32) -> i32 {
    if speed > 0 {
        ticks * (1 + speed)
    } else {
        ticks / (1 - speed)
    }
}

/// Euclidean modulo (always non-negative).
pub fn modulo(x: i32, m: i32) -> i32 {
    let r = x % m;
    if r < 0 { r + m } else { r }
}

// ---------------------------------------------------------------------------
// Pattern data helpers
// ---------------------------------------------------------------------------

/// Read a 24-bit pattern data word from a track frame.
fn get_pattern_data(track: &Track, frame: u32) -> u32 {
    let mut data = 0u32;
    let start = (frame as usize) * TRACK_PATTERNS_SIZE as usize;
    for b in 0..TRACK_PATTERNS_SIZE as usize {
        data |= (track.data[start + b] as u32) << (BITS_IN_BYTE * b);
    }
    data
}

/// Get the pattern id for a given frame and channel.
pub fn get_pattern_id(track: &Track, frame: u32, channel: u32) -> i32 {
    let pd = get_pattern_data(track, frame);
    ((pd >> (channel * TRACK_PATTERN_BITS)) & TRACK_PATTERN_MASK) as i32
}

/// Set the pattern id for a given frame and channel.
pub fn set_pattern_id(track: &mut Track, frame: u32, channel: u32, pattern: i32) {
    let mut pd = get_pattern_data(track, frame);
    let shift = channel * TRACK_PATTERN_BITS;
    pd &= !(TRACK_PATTERN_MASK << shift);
    pd |= (pattern as u32 & TRACK_PATTERN_MASK) << shift;
    let start = (frame as usize) * TRACK_PATTERNS_SIZE as usize;
    for b in 0..TRACK_PATTERNS_SIZE as usize {
        track.data[start + b] = ((pd >> (b * BITS_IN_BYTE)) & 0xff) as u8;
    }
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

/// Pack `Rgb` into a u32 ARGB value.
pub fn rgba(c: &Rgb) -> u32 {
    0xff << 24 | (c.b as u32) << 16 | (c.g as u32) << 8 | c.r as u32
}

/// Find the nearest color index in a palette using Euclidean distance.
pub fn nearest_color(palette: &[Rgb], color: &Rgb) -> u32 {
    let mut min = u32::MAX;
    let mut nearest = 0;
    for (i, rgb) in palette.iter().enumerate() {
        let dr = color.r as i32 - rgb.r as i32;
        let dg = color.g as i32 - rgb.g as i32;
        let db = color.b as i32 - rgb.b as i32;
        let dst = (dr * dr + dg * dg + db * db) as u32;
        if dst < min {
            min = dst;
            nearest = i as u32;
        }
    }
    nearest
}

/// Convert a palette to a blit palette in the given pixel format.
pub fn palette_blit(src: &Palette, fmt: PixelFormat) -> BlitPal {
    let mut pal = BlitPal { data: [0u32; TIC_PALETTE_SIZE] };
    for (i, c) in src.colors.iter().enumerate() {
        pal.data[i] = match fmt {
            PixelFormat::Bgra8888 => {
                0xff << 24 | (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32
            }
            PixelFormat::Rgba8888 => {
                0xff << 24 | (c.b as u32) << 16 | (c.g as u32) << 8 | c.r as u32
            }
            PixelFormat::Abgr8888 => {
                (c.r as u32) << 0 | (c.g as u32) << 8 | (c.b as u32) << 16 | 0xff << 24
            }
            PixelFormat::Argb8888 => {
                (c.b as u32) << 0 | (c.g as u32) << 8 | (c.r as u32) << 16 | 0xff << 24
            }
        };
    }
    pal
}

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

/// Check if `name` ends with `ext`.
pub fn has_ext(name: &str, ext: &str) -> bool {
    name.ends_with(ext)
}

/// Get the SFX id value from a track row.
pub fn get_track_row_sfx(row: &TrackRow) -> i32 {
    row.sfx() as i32
}

/// Set the SFX id in a track row.
pub fn set_track_row_sfx(row: &mut TrackRow, sfx: i32) {
    row.set_sfx(sfx as u32);
}

// ---------------------------------------------------------------------------
// Buffer helpers
// ---------------------------------------------------------------------------

/// Check if a buffer is all zeros.
pub fn buffer_empty(buf: &[u8]) -> bool {
    buf.iter().all(|&b| b == 0)
}

/// Check if all nibbles in a buffer are equal.
pub fn buffer_flat4(buf: &[u8]) -> bool {
    if buf.is_empty() {
        return true;
    }
    let first = buf[0] & 0x0f;
    let fill = first | (first << 4);
    buf.iter().all(|&b| b == fill)
}

/// Check if a waveform is a noise waveform (all nibbles = 0xF).
pub fn is_noise(wave: &Waveform) -> bool {
    buffer_flat4(&wave.data) && wave.data[0] % 0xff == 0
}

// ---------------------------------------------------------------------------
// Hex conversion helpers
// ---------------------------------------------------------------------------

/// Convert a byte buffer to a hex string.
///
/// If `flip`, swaps each pair of hex characters.
pub fn buf2str(data: &[u8], flip: bool) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for &b in data {
        let hi = hex_nibble(b >> 4);
        let lo = hex_nibble(b & 0x0f);
        if flip {
            s.push(lo);
            s.push(hi);
        } else {
            s.push(hi);
            s.push(lo);
        }
    }
    s
}

/// Convert a hex string to a byte buffer.
///
/// If `flip`, reads each hex pair in reverse order.
pub fn str2buf(s: &str, flip: bool) -> Vec<u8> {
    let chars: Vec<char> = s.chars().collect();
    let mut buf = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks(2) {
        if pair.len() < 2 {
            break;
        }
        let (hi, lo) = if flip {
            (pair[1], pair[0])
        } else {
            (pair[0], pair[1])
        };
        let val = (from_hex(hi) << 4) | from_hex(lo);
        buf.push(val);
    }
    buf
}

fn hex_nibble(v: u8) -> char {
    if v < 10 {
        (b'0' + v) as char
    } else {
        (b'a' + v - 10) as char
    }
}

fn from_hex(c: char) -> u8 {
    match c {
        '0'..='9' => c as u8 - b'0',
        'a'..='f' => c as u8 - b'a' + 10,
        'A'..='F' => c as u8 - b'A' + 10,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Meta-tag parser
// ---------------------------------------------------------------------------

/// Extract a meta-tag value from source code.
///
/// Searches for `comment tag:` (or `tag:` if comment is None) and returns
/// the rest of that line with leading/trailing whitespace trimmed.
pub fn metatag<'a>(code: &'a str, tag: &str, comment: Option<&str>) -> &'a str {
    let needle = if let Some(c) = comment {
        // "comment tag:"
        let mut s = String::with_capacity(c.len() + 1 + tag.len() + 1);
        s.push_str(c);
        s.push(' ');
        s.push_str(tag);
        s.push(':');
        s
    } else {
        // "tag:"
        let mut s = String::with_capacity(tag.len() + 1);
        s.push_str(tag);
        s.push(':');
        s
    };

    if let Some(start) = code.find(&needle) {
        let after = &code[start + needle.len()..];
        let end = after.find('\n').unwrap_or(after.len());
        after[..end].trim()
    } else {
        ""
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- math ---

    #[test]
    fn sfx_pos_positive_speed() {
        assert_eq!(sfx_pos(2, 10), 30); // 10 * (1+2)
    }

    #[test]
    fn sfx_pos_zero_speed() {
        assert_eq!(sfx_pos(0, 10), 10); // 10 * 1
    }

    #[test]
    fn sfx_pos_negative_speed() {
        assert_eq!(sfx_pos(-3, 100), 25); // 100 / (1+3) = 25
    }

    #[test]
    fn modulo_positive() {
        assert_eq!(modulo(7, 5), 2);
    }

    #[test]
    fn modulo_negative() {
        assert_eq!(modulo(-7, 5), 3); // -7 % 5 = -2 → +5 = 3
    }

    // --- buffer ---

    #[test]
    fn buffer_empty_works() {
        assert!(buffer_empty(&[0, 0, 0]));
        assert!(!buffer_empty(&[0, 1, 0]));
        assert!(buffer_empty(&[]));
    }

    #[test]
    fn buffer_flat4_works() {
        assert!(buffer_flat4(&[0x00, 0x00]));
        assert!(buffer_flat4(&[0x11, 0x11]));
        assert!(buffer_flat4(&[0xFF, 0xFF]));
        assert!(!buffer_flat4(&[0x12, 0x12]));
        assert!(buffer_flat4(&[]));
    }

    // --- color ---

    #[test]
    fn rgba_packing() {
        let c = Rgb { r: 0x12, g: 0x34, b: 0x56 };
        assert_eq!(rgba(&c), 0xff563412);
    }

    #[test]
    fn nearest_color_exact() {
        let pal = [
            Rgb { r: 0, g: 0, b: 0 },
            Rgb { r: 255, g: 0, b: 0 },
            Rgb { r: 0, g: 255, b: 0 },
        ];
        assert_eq!(nearest_color(&pal, &Rgb { r: 0, g: 0, b: 0 }), 0);
        assert_eq!(nearest_color(&pal, &Rgb { r: 250, g: 0, b: 0 }), 1);
        assert_eq!(nearest_color(&pal, &Rgb { r: 0, g: 250, b: 0 }), 2);
    }

    #[test]
    fn nearest_color_approx() {
        let pal = [
            Rgb { r: 0, g: 0, b: 0 },
            Rgb { r: 255, g: 255, b: 255 },
        ];
        assert_eq!(nearest_color(&pal, &Rgb { r: 128, g: 128, b: 128 }), 1);
    }

    #[test]
    fn palette_blit_bgra() {
        let mut pal = Palette { colors: [Rgb { r: 0, g: 0, b: 0 }; 16] };
        pal.colors[0] = Rgb { r: 0x12, g: 0x34, b: 0x56 };
        let blit = palette_blit(&pal, PixelFormat::Bgra8888);
        // C code: *dst++ = b; *dst++ = g; *dst++ = r; *dst++ = 0xff
        // Little-endian u32 = 0xff123456
        assert_eq!(blit.data[0], 0xff123456);
    }

    // --- string ---

    #[test]
    fn has_ext_matches() {
        assert!(has_ext("hello.tic", ".tic"));
        assert!(!has_ext("hello.tic", ".txt"));
    }

    // --- hex conversion ---

    #[test]
    fn buf2str_normal() {
        assert_eq!(buf2str(&[0xAB, 0xCD], false), "abcd");
        assert_eq!(buf2str(&[0x01, 0x02], false), "0102");
    }

    #[test]
    fn buf2str_flip() {
        assert_eq!(buf2str(&[0xAB], true), "ba");
    }

    #[test]
    fn str2buf_normal() {
        assert_eq!(str2buf("abcd", false), vec![0xab, 0xcd]);
    }

    #[test]
    fn str2buf_flip() {
        assert_eq!(str2buf("ba", true), vec![0xab]);
    }

    #[test]
    fn str2buf_roundtrip() {
        let data = [0x12, 0x34, 0xAB, 0xCD];
        let s = buf2str(&data, false);
        assert_eq!(str2buf(&s, false), data);
    }

    // --- pattern ---

    #[test]
    fn pattern_id_rw() {
        let mut track = Track {
            data: [0u8; MUSIC_FRAMES as usize * TRACK_PATTERNS_SIZE as usize],
            tempo: 0,
            rows: 64,
            speed: 0,
        };
        // Write pattern 5 to frame 0, channel 1
        set_pattern_id(&mut track, 0, 1, 5);
        assert_eq!(get_pattern_id(&track, 0, 1), 5);
        // Other channels unaffected
        assert_eq!(get_pattern_id(&track, 0, 0), 0);
        assert_eq!(get_pattern_id(&track, 0, 2), 0);
    }

    // --- SFX row ---

    #[test]
    fn track_row_sfx_rw() {
        let mut row = TrackRow([0; 3]);
        row.set_sfx(42);
        assert_eq!(row.sfx(), 42);
    }

    #[test]
    fn track_row_sfx_clamp() {
        let mut row = TrackRow([0; 3]);
        row.set_sfx(100); // SFX_COUNT-1 = 63
        assert_eq!(row.sfx(), 63);
    }

    #[test]
    fn track_row_sfx_max() {
        let mut row = TrackRow([0; 3]);
        row.set_sfx(63);
        assert_eq!(row.sfx(), 63);
    }

    // --- meta-tag ---

    #[test]
    fn metatag_basic() {
        let code = "-- script: myscript\n-- title: hello\n";
        assert_eq!(metatag(code, "script", Some("--")), "myscript");
    }

    #[test]
    fn metatag_no_comment() {
        let code = "script: myscript\ntitle: hello\n";
        assert_eq!(metatag(code, "script", None), "myscript");
    }

    #[test]
    fn metatag_missing() {
        let code = "nothing here\n";
        assert_eq!(metatag(code, "script", Some("--")), "");
    }

    #[test]
    fn metatag_trims_whitespace() {
        let code = "-- title:   my game  \n";
        assert_eq!(metatag(code, "title", Some("--")), "my game");
    }

    // --- noise ---

    #[test]
    fn is_noise_true() {
        let mut w = Waveform { data: [0; WAVE_SIZE] };
        // All nibbles = 0xF → all bytes = 0xFF
        for b in &mut w.data {
            *b = 0xFF;
        }
        assert!(is_noise(&w));
    }
}
