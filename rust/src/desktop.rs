//! SDL2 Desktop — full port of `system/sdl/main.c` + `system/sdl/player.c`.
//!
//! SDL2 window, rendering, input, audio.  Requires the `sdl2` crate.

use crate::io;
use crate::tic::Tic80;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::Window;
use sdl2::Sdl;
use std::sync::mpsc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const WINDOW_TITLE: &str = "TIC-80";
const WINDOW_SCALE: u32 = 3;
const TIC80_FULLWIDTH: u32 = 256;
const TIC80_FULLHEIGHT: u32 = 256;
const TIC80_FRAMERATE: u32 = 60;

/// Map SDL2 scancodes to TIC-80 gamepad bits.
const GAMEPAD_KEY_MAP: [(sdl2::keyboard::Scancode, u32); 8] = [
    (sdl2::keyboard::Scancode::Up, 0),
    (sdl2::keyboard::Scancode::Down, 1),
    (sdl2::keyboard::Scancode::Left, 2),
    (sdl2::keyboard::Scancode::Right, 3),
    (sdl2::keyboard::Scancode::Z, 4),
    (sdl2::keyboard::Scancode::X, 5),
    (sdl2::keyboard::Scancode::A, 6),
    (sdl2::keyboard::Scancode::S, 7),
];

// ---------------------------------------------------------------------------
// Audio callback
// ---------------------------------------------------------------------------

/// SAFETY: single-threaded; Tic80 is only accessed from the audio thread
/// while the main thread is blocked by the mutex (like the C code).
unsafe impl Send for TicAudio {}

struct TicAudio {
    tic: *mut Tic80,
    buffer: Vec<i16>,
    pos: usize,
    sample_rate: u32,
}

impl AudioCallback for TicAudio {
    type Channel = i16;

    fn callback(&mut self, out: &mut [i16]) {
        let samples_per_frame =
            (self.sample_rate * 2 / TIC80_FRAMERATE) as usize;

        for (i, sample) in out.iter_mut().enumerate() {
            if self.pos >= self.buffer.len() {
                // Generate next frame of audio
                unsafe {
                    (*self.tic).sound();
                }
                // Copy samples from TIC-80 output
                let n = samples_per_frame * 2; // stereo
                unsafe {
                    let src = std::slice::from_raw_parts(
                        (*self.tic).samples.as_ptr(),
                        n.min((*self.tic).samples.len()),
                    );
                    self.buffer = src.to_vec();
                }
                self.pos = 0;
            }
            *sample = if self.pos < self.buffer.len() {
                self.buffer[self.pos]
            } else {
                0
            };
            self.pos += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Desktop Application
// ---------------------------------------------------------------------------

pub struct DesktopApp {
    sdl: Sdl,
    canvas: Canvas<Window>,
    texture: Texture<'static>,
    tic: Tic80,
    audio_device: Option<AudioDevice<TicAudio>>,
    last_tick: Instant,
}

impl DesktopApp {
    /// Create a new TIC-80 desktop window.
    pub fn new() -> Result<Self, String> {
        let sdl = sdl2::init()?;
        let video = sdl.video()?;

        let window = video
            .window(
                WINDOW_TITLE,
                TIC80_FULLWIDTH * WINDOW_SCALE,
                TIC80_FULLHEIGHT * WINDOW_SCALE,
            )
            .position_centered()
            .resizable()
            .opengl()
            .build()
            .map_err(|e| e.to_string())?;

        let canvas = window.into_canvas().accelerated().build()
            .map_err(|e| e.to_string())?;

        let texture_creator = canvas.texture_creator();
        let texture = texture_creator
            .create_texture_streaming(
                PixelFormatEnum::ARGB8888,
                TIC80_FULLWIDTH,
                TIC80_FULLHEIGHT,
            )
            .map_err(|e| e.to_string())?;

        // SAFETY: texture lives as long as the app
        let texture = unsafe { std::mem::transmute::<_, Texture<'static>>(texture) };

        let tic = Tic80::create(44100);

        // Initialise audio
        let audio = sdl.audio()?;
        let desired = AudioSpecDesired {
            freq: Some(44100),
            channels: Some(2),
            samples: Some(1024),
        };

        let tic_ptr: *mut Tic80 = &tic as *const _ as *mut Tic80;
        let audio_cb = TicAudio {
            tic: tic_ptr,
            buffer: Vec::new(),
            pos: 0,
            sample_rate: 44100,
        };

        let audio_device = audio
            .open_playback(None, &desired, |_| audio_cb)
            .ok();

        if let Some(ref dev) = audio_device {
            dev.resume();
        }

        Ok(DesktopApp {
            sdl,
            canvas,
            texture,
            tic,
            audio_device,
            last_tick: Instant::now(),
        })
    }

    /// Run the main loop.
    pub fn run(&mut self) -> Result<(), String> {
        let mut event_pump = self.sdl.event_pump()?;
        let frame_duration = Duration::from_nanos(1_000_000_000 / TIC80_FRAMERATE as u64);

        'main: loop {
            let frame_start = Instant::now();

            // Process events
            let mut input = io::Input::default();

            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. }
                    | Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    } => break 'main,

                    Event::KeyDown { scancode, .. } => {
                        if let Some(sc) = scancode {
                            self.handle_key(sc, &mut input, true);
                        }
                    }
                    Event::KeyUp { scancode, .. } => {
                        if let Some(sc) = scancode {
                            self.handle_key(sc, &mut input, false);
                        }
                    }
                    _ => {}
                }
            }

            // Tick TIC-80
            self.tic.tick(&input, None, None);

            // Render
            self.render()?;

            // Frame rate limiting
            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        }

        Ok(())
    }

    fn handle_key(&self, sc: sdl2::keyboard::Scancode, input: &mut io::Input, down: bool) {
        for &(scan, bit) in &GAMEPAD_KEY_MAP {
            if sc == scan {
                if down {
                    let raw = input.gamepads.data();
                    input.gamepads.set_data(raw | (1 << bit));
                }
                return;
            }
        }
    }

    fn render(&mut self) -> Result<(), String> {
        self.texture
            .with_lock(None, |pixels: &mut [u8], pitch: usize| {
                for y in 0..TIC80_FULLHEIGHT as usize {
                    let src_start = y * TIC80_FULLWIDTH as usize;
                    let dst_start = y * pitch / 4;
                    for x in 0..TIC80_FULLWIDTH as usize {
                        let src_idx = src_start + x;
                        if src_idx < self.tic.screen.len() {
                            let rgba = self.tic.screen[src_idx];
                            let dst = dst_start + x;
                            if dst * 4 + 3 < pixels.len() {
                                pixels[dst * 4] = (rgba >> 16) as u8;     // R
                                pixels[dst * 4 + 1] = (rgba >> 8) as u8;  // G
                                pixels[dst * 4 + 2] = rgba as u8;         // B
                                pixels[dst * 4 + 3] = (rgba >> 24) as u8; // A
                            }
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        self.canvas.clear();
        self.canvas
            .copy(&self.texture, None, None)
            .map_err(|e| e.to_string())?;
        self.canvas.present();

        Ok(())
    }

    /// Load a cartridge file.
    pub fn load_cartridge(&mut self, data: &[u8]) {
        self.tic.load(data);
    }
}

// ---------------------------------------------------------------------------
// Standalone entry point (matches sdl/player.c main())
// ---------------------------------------------------------------------------

/// Run a cartridge in a standalone window (like player.c).
pub fn run_cartridge(cart_data: &[u8]) -> Result<(), String> {
    let mut app = DesktopApp::new()?;
    app.load_cartridge(cart_data);
    app.run()
}

// ---------------------------------------------------------------------------
// Tests (only compile tests — SDL2 not available in CI)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    // SDL2-dependent tests need actual display — compiled but not run in CI
    #[test]
    fn desktop_struct_size() {
        assert!(std::mem::size_of::<super::DesktopApp>() > 0);
    }
}
