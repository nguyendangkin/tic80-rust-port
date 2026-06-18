//! Extra modules: ext/fft.c, ext/gif.c, ext/png.c,
//! system/libretro, system/n3ds, system/nswitch
//!
//! These depend on external C libraries (miniaudio, gif, png,
//! libretro, 3ds, switch SDKs).

// ===========================================================================
// ext/fft.c — FFT with miniaudio microphone input
// ===========================================================================

pub mod fft {
    // Uses kiss_fft (already ported) + miniaudio C library
    // The C fft.c wraps miniaudio for real-time microphone capture
    pub struct FftCapture {
        pub enabled: bool,
    }
    impl FftCapture {
        pub fn new() -> Self { FftCapture { enabled: false } }
        pub fn open(&mut self) -> bool {
            eprintln!("FFT capture requires miniaudio C library");
            false
        }
        pub fn close(&mut self) { self.enabled = false; }
    }
}

// ===========================================================================
// ext/gif.c — GIF recording (used for screen recording)
// ===========================================================================

pub mod gif {
    // Can use the `gif` crate instead of C msf_gif.h
    pub struct GifRecorder {
        pub frames: Vec<Vec<u8>>,
        pub width: u32,
        pub height: u32,
    }
    impl GifRecorder {
        pub fn new(w: u32, h: u32) -> Self {
            GifRecorder { frames: Vec::new(), width: w, height: h }
        }
        pub fn add_frame(&mut self, pixels: &[u8]) {
            self.frames.push(pixels.to_vec());
        }
        pub fn encode(&self) -> Vec<u8> {
            // Would use `gif` crate to encode
            Vec::new()
        }
    }
}

// ===========================================================================
// ext/png.c — PNG encode/decode (cartridge images)
// ===========================================================================

pub mod png {
    // Can use the `png` crate instead of C libpng
    pub struct PngImage {
        pub width: u32,
        pub height: u32,
        pub pixels: Vec<u8>,
    }
    impl PngImage {
        pub fn decode(_data: &[u8]) -> Option<Self> {
            // Would use `png` crate decoder
            None
        }
        pub fn encode(&self) -> Vec<u8> {
            // Would use `png` crate encoder
            Vec::new()
        }
    }
}

// ===========================================================================
// system/libretro — RetroArch core
// ===========================================================================

pub mod libretro {
    // Requires libretro.h C header
    // Core callbacks: retro_init, retro_deinit, retro_run, retro_load_game, etc.
    pub struct LibretroCore;
    impl LibretroCore {
        pub fn new() -> Self { LibretroCore }
        pub fn run(&mut self) { /* retro_run callback */ }
    }
}

// ===========================================================================
// system/n3ds — Nintendo 3DS
// ===========================================================================

pub mod n3ds {
    pub struct N3dsApp;
    impl N3dsApp {
        pub fn new() -> Self { N3dsApp }
        pub fn run(&mut self) {
            // Requires citro3d, devoptab, etc.
        }
    }
}

// ===========================================================================
// system/nswitch — Nintendo Switch
// ===========================================================================

pub mod nswitch {
    pub struct SwitchApp;
    impl SwitchApp {
        pub fn new() -> Self { SwitchApp }
        pub fn run(&mut self) {
            // Requires switch.h (libnx)
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gif_recorder() {
        let mut g = gif::GifRecorder::new(256, 256);
        g.add_frame(&[0u8; 256*256*4]);
        assert_eq!(g.frames.len(), 1);
    }
    #[test]
    fn fft_capture() {
        let f = fft::FftCapture::new();
        assert!(!f.enabled);
    }
    #[test]
    fn platform_types() {
        let _ = libretro::LibretroCore::new();
        let _ = n3ds::N3dsApp::new();
        let _ = nswitch::SwitchApp::new();
    }
}
