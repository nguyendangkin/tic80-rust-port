//! zlib compress/uncompress wrappers.
//!
//! Port of TIC-80's `src/zip.c`.
//!
//! Uses the pure-Rust `flate2` crate (via miniz-oxide) instead of
//! linking to system zlib.

use flate2::write::ZlibEncoder;
use flate2::read::ZlibDecoder;
use flate2::Compression;
use std::io::{Write, Read};

/// Compress `source` into `dest`.
///
/// Returns the number of bytes written to `dest`, or `0` on failure.
///
/// Matches the original C API which uses zlib `compress2` with
/// `Z_BEST_COMPRESSION` (level 9).
pub fn tic_tool_zip(dest: &mut [u8], source: &[u8]) -> u32 {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    if encoder.write_all(source).is_err() {
        return 0;
    }
    let compressed = match encoder.finish() {
        Ok(data) => data,
        Err(_) => return 0,
    };

    if compressed.len() > dest.len() {
        return 0;
    }

    let len = compressed.len();
    dest[..len].copy_from_slice(&compressed);
    len as u32
}

/// Decompress `source` into `dest`.
///
/// Returns the number of bytes written to `dest`, or `0` on failure.
pub fn tic_tool_unzip(dest: &mut [u8], source: &[u8]) -> u32 {
    let mut decoder = ZlibDecoder::new(source);
    let mut tmp = Vec::with_capacity(dest.len());

    match decoder.read_to_end(&mut tmp) {
        Ok(size) => {
            if size > dest.len() {
                return 0;
            }
            dest[..size].copy_from_slice(&tmp);
            size as u32
        }
        Err(_) => 0,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_small() {
        let original = b"Hello, TIC-80! This is a test of zlib compression.";
        let mut compressed = vec![0u8; 1024];
        let compressed_size = tic_tool_zip(&mut compressed, original);

        assert!(compressed_size > 0, "compression failed");

        let mut decompressed = vec![0u8; 1024];
        let decompressed_size =
            tic_tool_unzip(&mut decompressed, &compressed[..compressed_size as usize]);

        assert_eq!(decompressed_size, original.len() as u32);
        assert_eq!(&decompressed[..decompressed_size as usize], original);
    }

    #[test]
    fn round_trip_large() {
        let original = vec![0xABu8; 4096];
        let mut compressed = vec![0u8; 4096];
        let compressed_size = tic_tool_zip(&mut compressed, &original);

        assert!(compressed_size > 0, "compression failed");

        let mut decompressed = vec![0u8; 4096];
        let decompressed_size =
            tic_tool_unzip(&mut decompressed, &compressed[..compressed_size as usize]);

        assert_eq!(decompressed_size, original.len() as u32);
        assert_eq!(&decompressed[..decompressed_size as usize], &original[..]);
    }

    #[test]
    fn empty_input() {
        let original = b"";
        let mut compressed = vec![0u8; 64];
        let compressed_size = tic_tool_zip(&mut compressed, original);

        assert!(compressed_size > 0, "empty compress should produce header");

        let mut decompressed = vec![0u8; 64];
        let decompressed_size =
            tic_tool_unzip(&mut decompressed, &compressed[..compressed_size as usize]);

        assert_eq!(decompressed_size, 0);
    }

    #[test]
    fn dest_too_small() {
        let original = b"this data needs compression!";
        let mut compressed = vec![0u8; 2];
        let result = tic_tool_zip(&mut compressed, original);
        assert_eq!(result, 0, "tiny dest should fail");
    }

    #[test]
    fn corrupted_input() {
        let corrupted = vec![0xFFu8; 32];
        let mut decompressed = vec![0u8; 64];
        let result = tic_tool_unzip(&mut decompressed, &corrupted);
        assert_eq!(result, 0, "corrupted data should fail");
    }

    #[test]
    fn realloc_compressible() {
        // Test with data that should compress significantly
        let original = vec![b'A'; 10000];
        let mut compressed = vec![0u8; 10000];
        let cs = tic_tool_zip(&mut compressed, &original);

        assert!(cs > 0, "compression failed");
        assert!(
            cs < 1000,
            "highly compressible data should compress to < 1000 bytes, got {}",
            cs
        );
    }
}
