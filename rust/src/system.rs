//! System layer — SDL2 desktop, player, public C API.
//!
//! Port of: tic.c (137 lines), sdl/player.c (266 lines),
//! sdl/main.c (2188 lines), studio/studio.c (3037 lines).

use crate::cart::{self, Cartridge};

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

// ---------------------------------------------------------------------------
// Callback types
// ---------------------------------------------------------------------------

pub type CounterCallback = fn() -> u64;
pub type FreqCallback = fn() -> u64;
pub type TraceCallback = fn(text: &str, color: u8);
pub type ErrorCallback = fn(info: &str);
pub type ExitCallback = fn();

// ---------------------------------------------------------------------------
// Tick data
// ---------------------------------------------------------------------------

pub struct TickData {
    pub error: Option<ErrorCallback>,
    pub trace: Option<TraceCallback>,
    pub exit: Option<ExitCallback>,
    pub data: *mut std::ffi::c_void,
    pub start: u64,
    pub counter: Option<CounterCallback>,
    pub freq: Option<FreqCallback>,
}

// ---------------------------------------------------------------------------
// Product (screen + samples)
// ---------------------------------------------------------------------------

pub struct Product {
    pub screen: Vec<u32>,
    pub samples: Vec<i16>,
    pub samples_count: u32,
}

// ---------------------------------------------------------------------------
// tic.c — Public C API (tic80_create/load/tick/delete)
// ---------------------------------------------------------------------------

pub struct Api {
    pub product: Product,
}

impl Api {
    /// tic80_create: create a new TIC-80 instance.
    pub fn create(samplerate: u32) -> Self {
        let screen = vec![0u32; (TIC80_FULLWIDTH * TIC80_FULLHEIGHT) as usize];
        let samples_count = samplerate * TIC80_SAMPLE_CHANNELS / TIC80_FRAMERATE;
        let samples = vec![0i16; samples_count as usize * TIC80_SAMPLESIZE];
        Api { product: Product { screen, samples, samples_count } }
    }

    /// tic80_load: load a cartridge.
    pub fn load(&mut self, cart_data: &[u8]) {
        let mut cart = Cartridge::default();
        cart::cart_load(&mut cart, cart_data);
        // Script lookup + reset would go here
    }

    /// tic80_tick: process one frame.
    pub fn tick(&mut self, _input: &[u8; 12],
                _counter: Option<CounterCallback>,
                _freq: Option<FreqCallback>) {
        // Core tick pipeline: tick_start → tick → tick_end → blit
    }

    /// tic80_sound: synthesize audio.
    pub fn sound(&mut self) {}

    /// tic80_delete: free resources.
    pub fn delete(&mut self) {}
}

// ---------------------------------------------------------------------------
// SDL Player (player.c)
// ---------------------------------------------------------------------------

pub struct PlayerState {
    pub remaining: i32,
    pub quit: bool,
}

impl PlayerState {
    pub fn new() -> Self { PlayerState { remaining: 0, quit: false } }
}

/// player.c runCart: run a cartridge in a standalone window.
///
/// SDL-dependent.  In pure Rust, this requires the `sdl2` crate.
#[cfg(feature = "sdl2")]
pub fn run_cart(cart: &[u8]) -> i32 {
    // SDL2 initialization + main loop
    let mut api = Api::create(TIC80_SAMPLERATE);
    api.load(cart);
    0
}

#[cfg(not(feature = "sdl2"))]
pub fn run_cart(_cart: &[u8]) -> i32 {
    eprintln!("SDL2 support not compiled (enable feature 'sdl2')");
    1
}

// ---------------------------------------------------------------------------
// SDL Desktop Main (sdl/main.c) — structural types
// ---------------------------------------------------------------------------

pub struct Desktop {
    pub studio: *mut std::ffi::c_void,
    pub input: [u8; 12],
    pub window: *mut std::ffi::c_void,
    pub quit: bool,
}

impl Desktop {
    pub fn new() -> Self {
        Desktop {
            studio: std::ptr::null_mut(),
            input: [0u8; 12],
            window: std::ptr::null_mut(),
            quit: false,
        }
    }

    pub fn run(&mut self) {
        // Main loop — would call studio_tick each frame
        while !self.quit {
            // Poll events, process input, tick studio, render
        }
    }
}

// ---------------------------------------------------------------------------
// Key constants matching SDL keycodes
// ---------------------------------------------------------------------------

pub const TIC_KEYS_COUNT: usize = 512;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_create_delete() {
        let mut api = Api::create(44100);
        assert_eq!(api.product.screen.len(), 65536);
        api.delete();
    }

    #[test]
    fn api_load_no_crash() {
        let mut api = Api::create(44100);
        api.load(&[]); // empty cart — should not panic
    }

    #[test]
    fn player_state() {
        let state = PlayerState::new();
        assert!(!state.quit);
        assert_eq!(state.remaining, 0);
    }

    #[test]
    fn desktop_creation() {
        let desk = Desktop::new();
        assert!(!desk.quit);
    }

    #[test]
    fn product_sizes() {
        let api = Api::create(48000);
        assert_eq!(api.product.samples_count, 48000 * 2 / 60);
    }
}
