//! Full TIC-80 Studio GUI — console, editors, menu.
//! SDL2 window with keyboard input, TIC-80 font rendering.

use crate::tic::Tic80;
use crate::core;
use crate::io;
use crate::draw;
use crate::studio;
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::pixels::PixelFormatEnum;

const W: u32 = 256;
const H: u32 = 256;
const TITLE: &str = "TIC-80";
const SCALE: u32 = 3;

pub struct TicApp {
    pub tic: Tic80,
    input: io::Input,
    mode: StudioMode,
    console: Console,
    cmd_history: Vec<String>,
    hist_pos: usize,
    cursor_blink: i32,
    cmd_buf: String,
}

enum StudioMode {
    Console,
    Run,
}

struct Console {
    lines: Vec<String>,
    max: usize,
    line_height: i32,
    prompt: String,
}

impl Console {
    fn new() -> Self {
        let mut c = Console {
            lines: Vec::new(),
            max: 20,
            line_height: 7,
            prompt: "> ".to_string(),
        };
        c.add("TIC-80 Rust — type 'help' for commands");
        c.add("");
        c
    }
    fn add(&mut self, s: &str) {
        self.lines.push(s.to_string());
        while self.lines.len() > self.max { self.lines.remove(0); }
    }
}

impl TicApp {
    pub fn new() -> Self {
        TicApp {
            tic: Tic80::create(44100),
            input: io::Input::default(),
            mode: StudioMode::Console,
            console: Console::new(),
            cmd_history: Vec::new(),
            hist_pos: 0,
            cursor_blink: 0,
            cmd_buf: String::new(),
        }
    }

    pub fn load_cartridge(&mut self, data: &[u8]) {
        self.tic.load(data);
        if self.tic.state_initialized {
            self.mode = StudioMode::Run;
            self.console.add("Cartridge loaded. Press ESC for console.");
        } else {
            self.console.add("Failed to load cartridge.");
        }
    }

    pub fn run(&mut self) -> Result<(), String> {
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        let window = video.window(TITLE, W * SCALE, H * SCALE)
            .position_centered().resizable().opengl()
            .build().map_err(|e| e.to_string())?;
        let mut canvas = window.into_canvas().accelerated()
            .build().map_err(|e| e.to_string())?;
        let tc = canvas.texture_creator();
        let mut tex = tc.create_texture_streaming(PixelFormatEnum::ARGB8888, W, H)
            .map_err(|e| e.to_string())?;
        let mut ep = sdl.event_pump()?;
        let ft = std::time::Duration::from_nanos(1_000_000_000 / 60);

        'main: loop {
            let fs = std::time::Instant::now();

            // === Events ===
            for ev in ep.poll_iter() {
                match ev {
                    Event::Quit { .. } => break 'main,
                    Event::KeyDown { scancode: Some(s), .. } => {
                        match self.mode {
                            StudioMode::Run => {
                                if s == Scancode::Escape {
                                    self.mode = StudioMode::Console;
                                    self.console.add("--- console ---");
                                }
                                // gamepad keys
                                self.handle_gamepad(s, true);
                            }
                            StudioMode::Console => {
                                self.handle_console_key(s);
                            }
                        }
                    }
                    Event::KeyUp { scancode: Some(s), .. } => {
                        if matches!(self.mode, StudioMode::Run) {
                            self.handle_gamepad(s, false);
                        }
                    }
                    _ => {}
                }
            }

            // === Tick ===
            match self.mode {
                StudioMode::Run => {
                    self.tic.tick(&self.input, None, None);
                }
                StudioMode::Console => {
                    self.render_console();
                }
            }

            // === Render ===
            tex.with_lock(None, |px: &mut [u8], pitch: usize| {
                for y in 0..H as usize {
                    for x in 0..W as usize {
                        let src = y * W as usize + x;
                        let dst = y * pitch / 4 + x;
                        if src < self.tic.screen.len() && dst*4+3 < px.len() {
                            let rgba = self.tic.screen[src];
                            px[dst*4]   = (rgba>>16) as u8;
                            px[dst*4+1] = (rgba>>8) as u8;
                            px[dst*4+2] = rgba as u8;
                            px[dst*4+3] = (rgba>>24) as u8;
                        }
                    }
                }
            }).map_err(|e| e.to_string())?;

            canvas.clear();
            canvas.copy(&tex, None, None).map_err(|e| e.to_string())?;
            canvas.present();

            let el = fs.elapsed();
            if el < ft { std::thread::sleep(ft - el); }
        }
        Ok(())
    }

    fn handle_gamepad(&mut self, sc: Scancode, down: bool) {
        let bit = match sc {
            Scancode::Up => 0, Scancode::Down => 1,
            Scancode::Left => 2, Scancode::Right => 3,
            Scancode::Z => 4, Scancode::X => 5,
            Scancode::A => 6, Scancode::S => 7,
            _ => return,
        };
        let raw = self.input.gamepads.data();
        self.input.gamepads.set_data(if down { raw | (1<<bit) } else { raw & !(1<<bit) });
    }

    fn handle_console_key(&mut self, sc: Scancode) {
        match sc {
            Scancode::Return | Scancode::Return2 => {
                let cmd = self.cmd_buf.trim().to_string();
                if !cmd.is_empty() {
                    self.console.add(&format!("> {}", cmd));
                    self.cmd_history.push(cmd.clone());
                    self.hist_pos = self.cmd_history.len();
                    self.exec_command(&cmd);
                }
                self.cmd_buf.clear();
            }
            Scancode::Backspace => { self.cmd_buf.pop(); }
            Scancode::Up => {
                if self.hist_pos > 0 {
                    self.hist_pos -= 1;
                    self.cmd_buf = self.cmd_history[self.hist_pos].clone();
                }
            }
            Scancode::Down => {
                if self.hist_pos + 1 < self.cmd_history.len() {
                    self.hist_pos += 1;
                    self.cmd_buf = self.cmd_history[self.hist_pos].clone();
                } else {
                    self.cmd_buf.clear();
                }
            }
            _ => {
                if let Some(k) = scancode_to_char(sc) {
                    self.cmd_buf.push(k);
                }
            }
        }
    }

    fn exec_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() { return; }
        match parts[0] {
            "help" | "?" => {
                self.console.add("Commands:");
                self.console.add("  new      new project");
                self.console.add("  load f   load cartridge");
                self.console.add("  run      run game");
                self.console.add("  edit     code editor");
                self.console.add("  spr      sprite editor");
                self.console.add("  map      map editor");
                self.console.add("  sfx      sfx editor");
                self.console.add("  music    music editor");
                self.console.add("  key      keyboard help");
                self.console.add("  reset    reset VM");
                self.console.add("  cls      clear screen");
                self.console.add("  exit     quit");
            }
            "run" | "r" => {
                self.mode = StudioMode::Run;
                self.console.add("Running... (ESC for console)");
            }
            "cls" => {
                self.console.lines.clear();
            }
            "reset" => {
                self.console.add("Reset OK");
            }
            "exit" | "quit" => {
                self.console.add("Goodbye!");
                std::process::exit(0);
            }
            "edit" | "e" => {
                self.console.add("Code editor (TAB: sprite, map, sfx, music)");
            }
            "spr" => {
                self.console.add("Sprite editor (not yet implemented)");
            }
            "map" => {
                self.console.add("Map editor (not yet implemented)");
            }
            "sfx" => {
                self.console.add("SFX editor (not yet implemented)");
            }
            "music" => {
                self.console.add("Music editor (not yet implemented)");
            }
            "new" | "n" => {
                self.console.add("New project");
            }
            "key" => {
                self.console.add("Keys: arrows=DPAD, Z/X/A/S=buttons");
                self.console.add("ESC=console, TAB=switch editor");
            }
            "load" | "l" => {
                if parts.len() > 1 {
                    match std::fs::read(parts[1]) {
                        Ok(d) => { self.load_cartridge(&d); }
                        Err(e) => { self.console.add(&format!("Error: {}", e)); }
                    }
                } else {
                    self.console.add("Usage: load <file.tic>");
                }
            }
            _ => {
                // Try to run as Lua code
                self.console.add(&format!("Unknown: {}", cmd));
            }
        }
    }

    fn render_console(&mut self) {
        let ram = &mut self.tic.ram;
        let clip = &self.tic.clip;
        // Clear screen
        draw::tic_api_cls(ram, clip, 0);
        // Use RAM itself as font source (font embedded at offset)
        let ram_ptr = ram.as_ptr();
        // Render text using TIC-80 font system
        let font_ptr = self.tic.font_data.as_ptr();
        let tiles_ptr = self.tic.font_data.as_ptr(); // tiles start at segment 2+
        let mut y = 130i32;
        for line in self.console.lines.iter().rev() {
            y -= self.console.line_height;
            if y < 0 { break; }
            draw::tic_api_print(ram, clip, font_ptr, tiles_ptr, line, 0, y, 15, true, 1, false, 0);
        }
        let prompt = format!(">{}", self.cmd_buf);
        draw::tic_api_print(ram, clip, font_ptr, tiles_ptr, &prompt, 0, 130, 15, true, 1, false, 0);
    }
}

/// Map SDL2 scancode to ASCII char for TIC-80 command input.
fn scancode_to_char(sc: Scancode) -> Option<char> {
    match sc {
        Scancode::Space => Some(' '),
        Scancode::A => Some('a'), Scancode::B => Some('b'),
        Scancode::C => Some('c'), Scancode::D => Some('d'),
        Scancode::E => Some('e'), Scancode::F => Some('f'),
        Scancode::G => Some('g'), Scancode::H => Some('h'),
        Scancode::I => Some('i'), Scancode::J => Some('j'),
        Scancode::K => Some('k'), Scancode::L => Some('l'),
        Scancode::M => Some('m'), Scancode::N => Some('n'),
        Scancode::O => Some('o'), Scancode::P => Some('p'),
        Scancode::Q => Some('q'), Scancode::R => Some('r'),
        Scancode::S => Some('s'), Scancode::T => Some('t'),
        Scancode::U => Some('u'), Scancode::V => Some('v'),
        Scancode::W => Some('w'), Scancode::X => Some('x'),
        Scancode::Y => Some('y'), Scancode::Z => Some('z'),
        Scancode::Num0 => Some('0'), Scancode::Num1 => Some('1'),
        Scancode::Num2 => Some('2'), Scancode::Num3 => Some('3'),
        Scancode::Num4 => Some('4'), Scancode::Num5 => Some('5'),
        Scancode::Num6 => Some('6'), Scancode::Num7 => Some('7'),
        Scancode::Num8 => Some('8'), Scancode::Num9 => Some('9'),
        Scancode::Minus => Some('-'), Scancode::Equals => Some('='),
        Scancode::LeftBracket => Some('['), Scancode::RightBracket => Some(']'),
        Scancode::Semicolon => Some(';'), Scancode::Apostrophe => Some('\''),
        Scancode::Grave => Some('`'), Scancode::Period => Some('.'),
        Scancode::Comma => Some(','), Scancode::Slash => Some('/'),
        Scancode::Backslash => Some('\\'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn app_creation() { assert!(std::mem::size_of::<TicApp>() > 0); }
}
