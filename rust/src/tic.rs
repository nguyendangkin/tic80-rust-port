//! Public C API — tic80_create/load/tick/sound/delete.
//!
//! Port of `src/tic.c`.  This is the main entry point that connects
//! the TIC-80 core engine to the outside world.

use crate::cart;
use crate::core;
use crate::draw;
use crate::io;
use crate::script;
use crate::sound;
use crate::tools;
use crate::system::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TIC80_SAMPLERATE: u32 = 44100;
pub const TIC80_FRAMERATE: u32 = 60;
pub const TIC80_FULLWIDTH: u32 = 256;
pub const TIC80_FULLHEIGHT: u32 = 256;
pub const TIC80_WIDTH: u32 = 240;
pub const TIC80_HEIGHT: u32 = 136;
pub const TIC80_SAMPLE_CHANNELS: u32 = 2;
pub const TIC80_SAMPLESIZE: usize = 4;
pub const TIC80_KEY_BUFFER: usize = 4;

// ---------------------------------------------------------------------------
// RAM buffer size
// ---------------------------------------------------------------------------

const TIC_RAM_SIZE: usize = 256 * 1024;

// ---------------------------------------------------------------------------
// TIC-80 instance
// ---------------------------------------------------------------------------

pub struct Tic80 {
    pub ram: Vec<u8>,
    pub screen: Vec<u32>,
    pub samples: Vec<i16>,
    pub samples_count: u32,
    pub clip: draw::ClipRect,
    pub gamepad_prev: io::Gamepads,
    pub gamepad_holds: [u32; 32],
    pub keyboard_prev: io::Keyboard,
    pub keyboard_holds: [u32; crate::io::tic_keys_count],
    pub mapping: io::Mapping,
    pub flags: Vec<u8>,
    pub state_initialized: bool,
    pub synced: u32,
    pub vbank_id: i32,
    pub vbank_mem: Vec<u8>,
}

impl Tic80 {
    /// tic80_create: allocate and initialise a new TIC-80 instance.
    pub fn create(samplerate: u32) -> Self {
        let ram = vec![0u8; TIC_RAM_SIZE];
        let screen = vec![0u32; (TIC80_FULLWIDTH * TIC80_FULLHEIGHT) as usize];
        let samples_count = samplerate * TIC80_SAMPLE_CHANNELS / TIC80_FRAMERATE;
        let samples = vec![0i16; samples_count as usize * TIC80_SAMPLESIZE];

        Tic80 {
            ram,
            screen,
            samples,
            samples_count,
            clip: draw::ClipRect {
                l: 0, t: 0,
                r: TIC80_WIDTH as i32,
                b: TIC80_HEIGHT as i32,
            },
            gamepad_prev: io::Gamepads::default(),
            gamepad_holds: [0u32; 32],
            keyboard_prev: io::Keyboard::default(),
            keyboard_holds: [0u32; crate::io::tic_keys_count],
            mapping: io::Mapping::default(),
            flags: vec![0u8; 1024],
            state_initialized: false,
            synced: 0,
            vbank_id: 0,
            vbank_mem: vec![0u8; 0xA000],
        }
    }

    /// tic80_load: load a cartridge from raw bytes.
    pub fn load(&mut self, data: &[u8]) {
        let mut cart = cart::Cartridge::default();
        cart::cart_load(&mut cart, data);

        // Find the right script engine
        // Simplified: just set initialized flag
        if !cart.code.is_empty() {
            self.state_initialized = true;
        }
    }

    /// tic80_tick: process one frame with input.
    pub fn tick(
        &mut self,
        input: &io::Input,
        _counter: Option<fn() -> u64>,
        _freq: Option<fn() -> u64>,
    ) {
        // 1. Start tick — process IO + sound
        let mut gp_holds = self.gamepad_holds;
        io::tick_io(
            &input.gamepads,
            &mut self.gamepad_prev,
            &mut gp_holds,
            &input.keyboard,
            &mut self.keyboard_prev,
            &mut self.keyboard_holds,
            &self.mapping,
        );
        self.gamepad_holds = gp_holds;

        // 2. Core tick — run user code (simplified: just mark synced)
        self.synced = core::SYNC_ALL;

        // 3. End tick — update previous state
        self.gamepad_prev = input.gamepads;

        // 4. Blit — render screen
        self.blit();
    }

    /// tic80_sound: synthesise audio samples.
    pub fn sound(&mut self) {
        // Audio synthesis via blip-buf stubs
        // In production, this calls the blip_buf C library
    }

    /// tic80_delete: free resources.
    pub fn delete(&mut self) {
        self.ram.clear();
        self.screen.clear();
        self.samples.clear();
    }

    // Internal: blit screen from RAM to pixel buffer
    fn blit(&mut self) {
        // Simplified screen rendering
        // In production, this iterates over the screen RAM and
        // converts 4bpp indexed pixels to RGBA using the palette
        for y in 0..TIC80_HEIGHT as usize {
            for x in 0..TIC80_WIDTH as usize {
                let addr = (y * TIC80_WIDTH as usize + x) as i32;
                let pix = core::peek4(&self.ram, addr);
                // Simple grayscale conversion
                let gray = (pix as u32) * 17;
                self.screen[y * TIC80_FULLWIDTH as usize + x] =
                    0xff000000 | (gray << 16) | (gray << 8) | gray;
            }
        }
    }
}

impl Drop for Tic80 {
    fn drop(&mut self) {
        self.delete();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_input() -> io::Input {
        io::Input::default()
    }

    #[test]
    fn create_delete() {
        let mut tic = Tic80::create(44100);
        assert_eq!(tic.screen.len(), 65536);
        assert_eq!(tic.ram.len(), TIC_RAM_SIZE);
        tic.delete();
    }

    #[test]
    fn tick_no_crash() {
        let mut tic = Tic80::create(44100);
        let input = test_input();
        tic.tick(&input, None, None);
    }

    #[test]
    fn load_empty_cart() {
        let mut tic = Tic80::create(44100);
        tic.load(&[]);
        assert!(!tic.state_initialized); // empty code → no init
    }

    #[test]
    fn blit_produces_output() {
        let mut tic = Tic80::create(44100);
        let input = test_input();
        // Write a pixel to RAM
        core::poke4(&mut tic.ram, 0, 0xF);
        tic.tick(&input, None, None);
        // Screen pixel 0 should be non-zero (since we wrote 0xF to RAM[0])
        assert!(tic.screen[0] != 0);
    }

    #[test]
    fn sound_no_crash() {
        let mut tic = Tic80::create(44100);
        tic.sound();
    }

    #[test]
    fn drop_doesnt_panic() {
        let tic = Tic80::create(44100);
        drop(tic); // explicit drop
    }

    #[test]
    fn framebuffer_size() {
        let tic = Tic80::create(44100);
        assert_eq!(tic.screen.len(), 256 * 256);
    }

    #[test]
    fn sample_buffer_size() {
        let tic = Tic80::create(48000);
        assert_eq!(tic.samples_count, 48000 * 2 / 60);
    }

    #[test]
    fn multiple_ticks() {
        let mut tic = Tic80::create(44100);
        let input = test_input();
        for _ in 0..10 {
            tic.tick(&input, None, None);
        }
    }
}
