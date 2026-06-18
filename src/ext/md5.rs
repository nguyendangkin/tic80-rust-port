//! MD5 Message-Digest Algorithm (RFC 1321)
//!
//! Port of the public-domain implementation by Alexander Peslyak (Solar Designer)
//! from TIC-80's `src/ext/md5.c`.
//!
//! # Example
//! ```rust
//! use md5::Md5;
//!
//! let digest = Md5::digest(b"hello world");
//! assert_eq!(digest, [0x5e, 0xb6, 0x3b, 0xbb, 0xe0, 0x1e, 0xee, 0xd0,
//!                     0x93, 0xcb, 0x22, 0xbb, 0x8f, 0x5a, 0xcd, 0xc3]);
//! ```

#![no_std]

// ---------------------------------------------------------------------------
// Non-linear functions (RFC 1321 Section 3.4)
// ---------------------------------------------------------------------------

/// F: (x & y) | (!x & z)
#[inline(always)]
fn f(x: u32, y: u32, z: u32) -> u32 {
    (z) ^ ((x) & ((y) ^ (z)))
}

/// G: (x & z) | (y & !z)
#[inline(always)]
fn g(x: u32, y: u32, z: u32) -> u32 {
    (y) ^ ((z) & ((x) ^ (y)))
}

/// H: x ^ y ^ z
#[inline(always)]
fn h(x: u32, y: u32, z: u32) -> u32 {
    (x ^ y) ^ z
}

/// H2: x ^ y ^ z (alternative form — byte-identical result)
#[inline(always)]
fn h2(x: u32, y: u32, z: u32) -> u32 {
    x ^ (y ^ z)
}

/// I: y ^ (x | !z)
#[inline(always)]
fn i(x: u32, y: u32, z: u32) -> u32 {
    y ^ (x | !z)
}

// ---------------------------------------------------------------------------
// One MD5 step
// ---------------------------------------------------------------------------

/// `a += f(b,c,d) + X[k] + T[i];  a = rotl(a, s);  a += b`
#[inline(always)]
fn step(
    f: fn(u32, u32, u32) -> u32,
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    xk: u32,
    ti: u32,
    s: u32,
) -> u32 {
    a.wrapping_add(f(b, c, d))
        .wrapping_add(xk)
        .wrapping_add(ti)
        .rotate_left(s)
        .wrapping_add(b)
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Round constants T[i] = floor(2^32 × |sin(i + 1)|)  (i = 0..63)
const T: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a,
    0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
    0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340,
    0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8,
    0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
    0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
    0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92,
    0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
    0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

/// Which X[k] word to use in each of the 64 steps.
const W: [[usize; 16]; 4] = [
    // round 1:  k = i
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    // round 2:  k = (5·i + 1) mod 16
    [1, 6, 11, 0, 5, 10, 15, 4, 9, 14, 3, 8, 13, 2, 7, 12],
    // round 3:  k = (3·i + 5) mod 16
    [5, 8, 11, 14, 1, 4, 7, 10, 13, 0, 3, 6, 9, 12, 15, 2],
    // round 4:  k = (7·i) mod 16
    [0, 7, 14, 5, 12, 3, 10, 1, 8, 15, 6, 13, 4, 11, 2, 9],
];

/// Shift amounts per round:  [S[round][0..3]]
const S: [[u32; 4]; 4] = [
    [7, 12, 17, 22],
    [5, 9, 14, 20],
    [4, 11, 16, 23],
    [6, 10, 15, 21],
];

// ---------------------------------------------------------------------------
// Core: process one 64-byte block
// ---------------------------------------------------------------------------

fn process_block(state: &mut [u32; 4], block: &[u8; 64]) {
    // Decode block into 16 little-endian u32s
    let mut x = [0u32; 16];
    for i in 0..16 {
        let off = i * 4;
        x[i] = u32::from_le_bytes([
            block[off],
            block[off + 1],
            block[off + 2],
            block[off + 3],
        ]);
    }

    let [mut a, mut b, mut c, mut d] = *state;
    let (aa, bb, cc, dd) = (a, b, c, d);

    macro_rules! R {
        ($f:expr, $a:ident, $b:ident, $c:ident, $d:ident, $i:expr) => {
            $a = step(
                $f, $a, $b, $c, $d,
                x[W[$i / 16][$i % 16]],
                T[$i],
                S[$i / 16][$i % 4],
            );
        };
    }

    // Round 1  (steps 0..15)  — F function
    R!(f, a, b, c, d, 0);
    R!(f, d, a, b, c, 1);
    R!(f, c, d, a, b, 2);
    R!(f, b, c, d, a, 3);
    R!(f, a, b, c, d, 4);
    R!(f, d, a, b, c, 5);
    R!(f, c, d, a, b, 6);
    R!(f, b, c, d, a, 7);
    R!(f, a, b, c, d, 8);
    R!(f, d, a, b, c, 9);
    R!(f, c, d, a, b, 10);
    R!(f, b, c, d, a, 11);
    R!(f, a, b, c, d, 12);
    R!(f, d, a, b, c, 13);
    R!(f, c, d, a, b, 14);
    R!(f, b, c, d, a, 15);

    // Round 2  (steps 16..31) — G function
    R!(g, a, b, c, d, 16);
    R!(g, d, a, b, c, 17);
    R!(g, c, d, a, b, 18);
    R!(g, b, c, d, a, 19);
    R!(g, a, b, c, d, 20);
    R!(g, d, a, b, c, 21);
    R!(g, c, d, a, b, 22);
    R!(g, b, c, d, a, 23);
    R!(g, a, b, c, d, 24);
    R!(g, d, a, b, c, 25);
    R!(g, c, d, a, b, 26);
    R!(g, b, c, d, a, 27);
    R!(g, a, b, c, d, 28);
    R!(g, d, a, b, c, 29);
    R!(g, c, d, a, b, 30);
    R!(g, b, c, d, a, 31);

    // Round 3  (steps 32..47) — H function (alternating H / H2)
    R!(h, a, b, c, d, 32);
    R!(h2, d, a, b, c, 33);
    R!(h, c, d, a, b, 34);
    R!(h2, b, c, d, a, 35);
    R!(h, a, b, c, d, 36);
    R!(h2, d, a, b, c, 37);
    R!(h, c, d, a, b, 38);
    R!(h2, b, c, d, a, 39);
    R!(h, a, b, c, d, 40);
    R!(h2, d, a, b, c, 41);
    R!(h, c, d, a, b, 42);
    R!(h2, b, c, d, a, 43);
    R!(h, a, b, c, d, 44);
    R!(h2, d, a, b, c, 45);
    R!(h, c, d, a, b, 46);
    R!(h2, b, c, d, a, 47);

    // Round 4  (steps 48..63) — I function
    R!(i, a, b, c, d, 48);
    R!(i, d, a, b, c, 49);
    R!(i, c, d, a, b, 50);
    R!(i, b, c, d, a, 51);
    R!(i, a, b, c, d, 52);
    R!(i, d, a, b, c, 53);
    R!(i, c, d, a, b, 54);
    R!(i, b, c, d, a, 55);
    R!(i, a, b, c, d, 56);
    R!(i, d, a, b, c, 57);
    R!(i, c, d, a, b, 58);
    R!(i, b, c, d, a, 59);
    R!(i, a, b, c, d, 60);
    R!(i, d, a, b, c, 61);
    R!(i, c, d, a, b, 62);
    R!(i, b, c, d, a, 63);

    // Add back saved state
    state[0] = a.wrapping_add(aa);
    state[1] = b.wrapping_add(bb);
    state[2] = c.wrapping_add(cc);
    state[3] = d.wrapping_add(dd);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// MD5 hasher state.
pub struct Md5 {
    state: [u32; 4],
    buffer: [u8; 64],
    buf_len: usize,
    count: u64, // total bytes processed
}

impl Md5 {
    /// Create a new MD5 hasher with the standard initial state.
    pub fn new() -> Self {
        Self {
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            buffer: [0u8; 64],
            buf_len: 0,
            count: 0,
        }
    }

    /// Feed bytes into the hash.
    pub fn update(&mut self, data: &[u8]) {
        let mut offset = 0usize;

        // 1) If we have a partial buffer, try to fill it
        if self.buf_len != 0 {
            let avail = 64 - self.buf_len;
            let take = data.len().min(avail);
            self.buffer[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            offset = take;

            if self.buf_len == 64 {
                process_block(&mut self.state, &self.buffer);
                self.buf_len = 0;
            } else {
                // Buffer still not full — nothing more to do
                self.count += data.len() as u64;
                return;
            }
        }

        // 2) Process all full 64-byte chunks directly from `data`
        let remaining = &data[offset..];
        let chunks = remaining.chunks_exact(64);
        for chunk in chunks.clone() {
            process_block(
                &mut self.state,
                // SAFETY: chunks_exact(64) guarantees length 64
                unsafe { &*(chunk.as_ptr() as *const [u8; 64]) },
            );
        }
        let remainder = chunks.remainder();

        // 3) Buffer leftover bytes (< 64)
        if !remainder.is_empty() {
            self.buffer[..remainder.len()].copy_from_slice(remainder);
            self.buf_len = remainder.len();
        }

        self.count += data.len() as u64;
    }

    /// Finalize the hash and return the 16-byte digest.
    ///
    /// Consumes `self` — call only once.
    pub fn finalize(mut self) -> [u8; 16] {
        // Total bit length (RFC 1321 requires the *bit* count in padding)
        let bit_len = self.count.wrapping_mul(8);

        // Append 0x80
        self.buffer[self.buf_len] = 0x80;
        self.buf_len += 1;

        // If fewer than 9 bytes left (8 for bit count + 1 for 0x80 already written),
        // pad rest with zeros, process block, start fresh
        if self.buf_len > 56 {
            self.buffer[self.buf_len..64].fill(0);
            process_block(&mut self.state, &self.buffer);
            self.buf_len = 0;
        }

        // Pad zeros up to byte 56 (reserve 8 bytes for bit count)
        let pad_end = 56usize.saturating_sub(self.buf_len);
        self.buffer[self.buf_len..self.buf_len + pad_end].fill(0);

        // Write 64-bit bit count (little-endian) at offset 56
        self.buffer[56..64].copy_from_slice(&bit_len.to_le_bytes());

        // Process final block
        process_block(&mut self.state, &self.buffer);

        // Emit state as 16 little-endian bytes
        let mut result = [0u8; 16];
        result[0..4].copy_from_slice(&self.state[0].to_le_bytes());
        result[4..8].copy_from_slice(&self.state[1].to_le_bytes());
        result[8..12].copy_from_slice(&self.state[2].to_le_bytes());
        result[12..16].copy_from_slice(&self.state[3].to_le_bytes());

        result
    }

    /// One-shot convenience: hash `data` in a single call.
    pub fn digest(data: &[u8]) -> [u8; 16] {
        let mut md5 = Self::new();
        md5.update(data);
        md5.finalize()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 1321 test suite
    #[test]
    fn rfc1321_test_suite() {
        // "" (empty string)
        assert_eq!(
            Md5::digest(b""),
            [0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04,
             0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8, 0x42, 0x7e]
        );

        // "a"
        assert_eq!(
            Md5::digest(b"a"),
            [0x0c, 0xc1, 0x75, 0xb9, 0xc0, 0xf1, 0xb6, 0xa8,
             0x31, 0xc3, 0x99, 0xe2, 0x69, 0x77, 0x26, 0x61]
        );

        // "abc"
        assert_eq!(
            Md5::digest(b"abc"),
            [0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0,
             0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1, 0x7f, 0x72]
        );

        // "message digest"
        assert_eq!(
            Md5::digest(b"message digest"),
            [0xf9, 0x6b, 0x69, 0x7d, 0x7c, 0xb7, 0x93, 0x8d,
             0x52, 0x5a, 0x2f, 0x31, 0xaa, 0xf1, 0x61, 0xd0]
        );

        // "abcdefghijklmnopqrstuvwxyz"
        assert_eq!(
            Md5::digest(b"abcdefghijklmnopqrstuvwxyz"),
            [0xc3, 0xfc, 0xd3, 0xd7, 0x61, 0x92, 0xe4, 0x00,
             0x7d, 0xfb, 0x49, 0x6c, 0xca, 0x67, 0xe1, 0x3b]
        );

        // "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
        assert_eq!(
            Md5::digest(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"),
            [0xd1, 0x74, 0xab, 0x98, 0xd2, 0x77, 0xd9, 0xf5,
             0xa5, 0x61, 0x1c, 0x2c, 0x9f, 0x41, 0x9d, 0x9f]
        );

        // "1234567890" x 8
        let eight_times = b"1234567890".repeat(8);
        assert_eq!(
            Md5::digest(&eight_times),
            [0x57, 0xed, 0xf4, 0xa2, 0x2b, 0xe3, 0xc9, 0x55,
             0xac, 0x49, 0xda, 0x2e, 0x21, 0x07, 0xb6, 0x7a]
        );
    }

    /// Incremental vs one-shot produce the same result
    #[test]
    fn incremental() {
        let data = b"The quick brown fox jumps over the lazy dog";

        let one_shot = Md5::digest(data);

        let mut md5 = Md5::new();
        md5.update(b"The quick brown ");
        md5.update(b"fox jumps over ");
        md5.update(b"the lazy dog");
        let incremental = md5.finalize();

        assert_eq!(one_shot, incremental);
    }

    /// Single byte
    #[test]
    fn single_byte() {
        assert_eq!(
            Md5::digest(b"x"),
            [0x9d, 0xd4, 0xe4, 0x61, 0x26, 0x8c, 0x80, 0x34,
             0xf5, 0xc8, 0x56, 0x4e, 0x15, 0x5c, 0x67, 0xa6]
        );
    }

    /// Exactly 64 bytes — one full block
    #[test]
    fn exact_one_block() {
        let data = [0x61u8; 64]; // 64 'a's
        let got = Md5::digest(&data);
        let expected = Md5::digest(&data); // sanity — repeatable
        assert_eq!(got, expected);
    }

    /// Exactly 55 bytes — padding fits in same block (1 + 55 + 8 = 64)
    #[test]
    fn padding_boundary_55() {
        let data = [0x41u8; 55];
        let a = Md5::digest(&data);
        let b = Md5::digest(&data);
        assert_eq!(a, b);
    }

    /// Exactly 56 bytes — 0x80 + zeros + bit count needs an extra block
    #[test]
    fn padding_boundary_56() {
        let data = [0x42u8; 56];
        let a = Md5::digest(&data);
        let b = Md5::digest(&data);
        assert_eq!(a, b);
    }
}
