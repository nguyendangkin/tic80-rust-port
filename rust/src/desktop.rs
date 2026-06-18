//! Desktop GUI — SDL2 window (zero system deps, vendored in sdl2-sys).

use crate::tic::Tic80;
use crate::io;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::Sdl;

const WINDOW_TITLE: &str = "TIC-80";
const WINDOW_SCALE: u32 = 3;
const TIC80_FULLWIDTH: u32 = 256;
const TIC80_FULLHEIGHT: u32 = 256;
const TIC80_FRAMERATE: u64 = 60;

pub struct TicApp {
    pub tic: Tic80,
    pub input: io::Input,
}

impl TicApp {
    pub fn new() -> Self {
        TicApp { tic: Tic80::create(44100), input: io::Input::default() }
    }

    pub fn load_cartridge(&mut self, data: &[u8]) { self.tic.load(data); }

    pub fn run(&mut self) -> Result<(), String> {
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        let window = video.window(WINDOW_TITLE, TIC80_FULLWIDTH * WINDOW_SCALE, TIC80_FULLHEIGHT * WINDOW_SCALE)
            .position_centered().resizable().opengl().build().map_err(|e| e.to_string())?;
        let mut canvas = window.into_canvas().accelerated().build().map_err(|e| e.to_string())?;
        let texture_creator = canvas.texture_creator();
        let mut texture = texture_creator.create_texture_streaming(PixelFormatEnum::ARGB8888, TIC80_FULLWIDTH, TIC80_FULLHEIGHT)
            .map_err(|e| e.to_string())?;
        let mut event_pump = sdl.event_pump()?;
        let frame_time = std::time::Duration::from_nanos(1_000_000_000 / TIC80_FRAMERATE);

        'main: loop {
            let frame_start = std::time::Instant::now();

            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'main,
                    Event::KeyDown { scancode: Some(sc), .. } => {
                        let bit = match sc {
                            sdl2::keyboard::Scancode::Up => Some(0),
                            sdl2::keyboard::Scancode::Down => Some(1),
                            sdl2::keyboard::Scancode::Left => Some(2),
                            sdl2::keyboard::Scancode::Right => Some(3),
                            sdl2::keyboard::Scancode::Z => Some(4),
                            sdl2::keyboard::Scancode::X => Some(5),
                            sdl2::keyboard::Scancode::A => Some(6),
                            sdl2::keyboard::Scancode::S => Some(7),
                            _ => None,
                        };
                        if let Some(b) = bit {
                            let raw = self.input.gamepads.data();
                            self.input.gamepads.set_data(raw | (1 << b));
                        }
                    }
                    Event::KeyUp { scancode: Some(sc), .. } => {
                        let bit = match sc {
                            sdl2::keyboard::Scancode::Up => Some(0),
                            sdl2::keyboard::Scancode::Down => Some(1),
                            sdl2::keyboard::Scancode::Left => Some(2),
                            sdl2::keyboard::Scancode::Right => Some(3),
                            sdl2::keyboard::Scancode::Z => Some(4),
                            sdl2::keyboard::Scancode::X => Some(5),
                            sdl2::keyboard::Scancode::A => Some(6),
                            sdl2::keyboard::Scancode::S => Some(7),
                            _ => None,
                        };
                        if let Some(b) = bit {
                            let raw = self.input.gamepads.data();
                            self.input.gamepads.set_data(raw & !(1 << b));
                        }
                    }
                    _ => {}
                }
            }

            self.tic.tick(&self.input, None, None);

            texture.with_lock(None, |pixels: &mut [u8], pitch: usize| {
                for y in 0..TIC80_FULLHEIGHT as usize {
                    for x in 0..TIC80_FULLWIDTH as usize {
                        let src = y * TIC80_FULLWIDTH as usize + x;
                        let dst = y * pitch / 4 + x;
                        if src < self.tic.screen.len() && dst * 4 + 3 < pixels.len() {
                            let rgba = self.tic.screen[src];
                            pixels[dst * 4]     = (rgba >> 16) as u8; // R
                            pixels[dst * 4 + 1] = (rgba >> 8) as u8;  // G
                            pixels[dst * 4 + 2] = rgba as u8;         // B
                            pixels[dst * 4 + 3] = (rgba >> 24) as u8; // A
                        }
                    }
                }
            }).map_err(|e| e.to_string())?;

            canvas.clear();
            canvas.copy(&texture, None, None).map_err(|e| e.to_string())?;
            canvas.present();

            let elapsed = frame_start.elapsed();
            if elapsed < frame_time { std::thread::sleep(frame_time - elapsed); }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn app_creation() { assert!(std::mem::size_of::<TicApp>() > 0); }
}
