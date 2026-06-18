//! Input handling — gamepad, keyboard, mouse.
//!
//! Port of TIC-80's `src/core/io.c`.
//!
//! Provides the core input API: `btn()`, `btnp()`, `key()`, `keyp()`,
//! `mouse()`, and the per-frame input processing `tick_io()`.

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TIC80_KEY_BUFFER: usize = 4;
pub const TIC_BUTTONS: usize = 8;
pub const TIC_GAMEPADS: usize = 4;
pub const TIC80_FULLWIDTH: u32 = 256;
pub const TIC80_FULLHEIGHT: u32 = 256;
pub const TIC80_WIDTH: u32 = 240;
pub const TIC80_HEIGHT: u32 = 136;
pub const TIC80_OFFSET_LEFT: i32 = ((TIC80_FULLWIDTH - TIC80_WIDTH) / 2) as i32;
pub const TIC80_OFFSET_TOP: i32 = ((TIC80_FULLHEIGHT - TIC80_HEIGHT) / 2) as i32;

pub const tic_key_unknown: u8 = 0;
pub const tic_keys_count: usize = 256; // size of tic_keycode enum

// ---------------------------------------------------------------------------
// Type stubs matching C layout
// ---------------------------------------------------------------------------

/// Single gamepad state (1 byte).
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Gamepad(pub u8);

/// Four gamepads packed as a u32.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Gamepads {
    pub first: Gamepad,
    pub second: Gamepad,
    pub third: Gamepad,
    pub fourth: Gamepad,
}

impl Gamepads {
    pub fn data(&self) -> u32 {
        u32::from_le_bytes([
            self.first.0,
            self.second.0,
            self.third.0,
            self.fourth.0,
        ])
    }

    pub fn set_data(&mut self, val: u32) {
        let bytes = val.to_le_bytes();
        self.first.0 = bytes[0];
        self.second.0 = bytes[1];
        self.third.0 = bytes[2];
        self.fourth.0 = bytes[3];
    }
}

/// Mouse state.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Mouse {
    pub x: u8,
    pub y: u8,
    pub rx: i8,
    pub ry: i8,
    pub btns: u16,
}

impl Mouse {
    pub fn left(&self) -> bool {
        self.btns & 1 != 0
    }
    pub fn middle(&self) -> bool {
        (self.btns >> 1) & 1 != 0
    }
    pub fn right(&self) -> bool {
        (self.btns >> 2) & 1 != 0
    }
    pub fn scrollx(&self) -> i16 {
        // 6-bit signed, sign-extended
        let val = (self.btns >> 3) & 0x3f;
        if val & 0x20 != 0 {
            (val | 0xffc0) as i16
        } else {
            val as i16
        }
    }
    pub fn scrolly(&self) -> i16 {
        let val = (self.btns >> 9) & 0x3f;
        if val & 0x20 != 0 {
            (val | 0xffc0) as i16
        } else {
            val as i16
        }
    }
    pub fn relative(&self) -> bool {
        (self.btns >> 15) & 1 != 0
    }
}

/// Keyboard state.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Keyboard {
    pub keys: [u8; TIC80_KEY_BUFFER],
    pub data: u32,
}

/// Combined input state.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Input {
    pub gamepads: Gamepads,
    pub mouse: Mouse,
    pub keyboard: Keyboard,
}

/// Key mapping (4 gamepads × 8 buttons).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Mapping {
    pub data: [u8; TIC_GAMEPADS * TIC_BUTTONS],
}

impl Default for Mapping {
    fn default() -> Self {
        Mapping {
            data: [0u8; TIC_GAMEPADS * TIC_BUTTONS],
        }
    }
}

// ---------------------------------------------------------------------------
// Core state (input-specific subset)
// ---------------------------------------------------------------------------

/// Per-button hold counter.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct GamepadState {
    pub previous: Gamepads,
    pub now: Gamepads,
    pub holds: [u32; 32], // sizeof(Gamepads) * 8
}

impl Default for GamepadState {
    fn default() -> Self {
        GamepadState {
            previous: Gamepads::default(),
            now: Gamepads::default(),
            holds: [0u32; 32],
        }
    }
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct KeyboardState {
    pub previous: Keyboard,
    pub now: Keyboard,
    pub holds: [u32; tic_keys_count],
}

impl Default for KeyboardState {
    fn default() -> Self {
        KeyboardState {
            previous: Keyboard::default(),
            now: Keyboard::default(),
            holds: [0u32; tic_keys_count],
        }
    }
}

// ---------------------------------------------------------------------------
// Point
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn is_key_pressed(keyboard: &Keyboard, key: u8) -> bool {
    keyboard.keys.iter().any(|&k| k == key)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// `btnp(index, hold, period)` — button pressed this frame with repeat.
pub fn btnp(
    gamepads: &Gamepads,
    previous: &Gamepads,
    holds: &[u32; 32],
    index: i32,
    hold: i32,
    period: i32,
) -> u32 {
    if index < 0 {
        // Any button pressed this frame
        return (!previous.data()) & gamepads.data();
    }

    if hold < 0 || period < 0 {
        // Specific button, no hold/period
        return ((!previous.data()) & gamepads.data()) & (1 << index);
    }

    let prev = if holds[index as usize] >= hold as u32 {
        if period > 0 && holds[index as usize] % period as u32 != 0 {
            previous.data()
        } else {
            0
        }
    } else {
        previous.data()
    };

    ((!prev) & gamepads.data()) & (1 << index)
}

/// `btn(index)` — button is currently held.
pub fn btn(gamepads: &Gamepads, index: i32) -> u32 {
    if index < 0 {
        gamepads.data()
    } else {
        gamepads.data() & (1 << index)
    }
}

/// `key(key)` — keyboard key is currently held.
pub fn key(keyboard: &Keyboard, key: u8) -> bool {
    if key > tic_key_unknown {
        is_key_pressed(keyboard, key)
    } else {
        keyboard.data != 0
    }
}

/// `keyp(key, hold, period)` — key pressed this frame with repeat.
pub fn keyp(
    keyboard: &Keyboard,
    previous: &Keyboard,
    holds: &[u32],
    key: u8,
    hold: i32,
    period: i32,
) -> bool {
    if key > tic_key_unknown {
        let prev_down = if hold >= 0 && period >= 0 && holds[key as usize] >= hold as u32 {
            if period > 0 && holds[key as usize] % period as u32 != 0 {
                is_key_pressed(previous, key)
            } else {
                false
            }
        } else {
            is_key_pressed(previous, key)
        };

        let down = is_key_pressed(keyboard, key);
        !prev_down && down
    } else {
        // Any key pressed this frame
        for i in 0..TIC80_KEY_BUFFER {
            let k = keyboard.keys[i];
            if k != 0 {
                let mut was_pressed = false;
                for p in 0..TIC80_KEY_BUFFER {
                    if previous.keys[p] == k {
                        was_pressed = true;
                        break;
                    }
                }
                if !was_pressed {
                    return true;
                }
            }
        }
        false
    }
}

/// `mouse()` — current mouse position (absolute or relative).
pub fn mouse(input: &Input) -> Point {
    if input.mouse.relative() {
        Point {
            x: input.mouse.rx as i32,
            y: input.mouse.ry as i32,
        }
    } else {
        Point {
            x: input.mouse.x as i32 - TIC80_OFFSET_LEFT,
            y: input.mouse.y as i32 - TIC80_OFFSET_TOP,
        }
    }
}

/// Per-frame input processing — called once per frame.
///
/// Updates hold counters based on current vs previous state.
pub fn tick_io(
    gamepads: &Gamepads,
    previous: &mut Gamepads,
    holds: &mut [u32; 32],
    keyboard: &Keyboard,
    keyboard_prev: &mut Keyboard,
    keyboard_holds: &mut [u32; tic_keys_count],
    mapping: &Mapping,
) {
    // Apply key mapping: mapped keys → gamepad bits
    let mut mapped = *gamepads;
    for i in 0..(TIC_GAMEPADS * TIC_BUTTONS) {
        let kc = mapping.data[i];
        if kc != 0 && is_key_pressed(keyboard, kc) {
            // Cast to *mut to modify the local copy
            let raw = mapped.data();
            mapped.set_data(raw | (1 << i));
        }
    }

    // Update gamepad hold counters
    for i in 0..32 {
        let mask = 1u32 << i;
        let prev_down = previous.data() & mask;
        let down = mapped.data() & mask;

        if prev_down != 0 && prev_down == down {
            holds[i] = holds[i].wrapping_add(1);
        } else {
            holds[i] = 0;
        }
    }

    // Update keyboard hold counters
    for i in 0..tic_keys_count {
        let prev_down = is_key_pressed(keyboard_prev, i as u8);
        let down = is_key_pressed(keyboard, i as u8);

        if prev_down && down {
            keyboard_holds[i] = keyboard_holds[i].wrapping_add(1);
        } else {
            keyboard_holds[i] = 0;
        }
    }

    // Save current as previous for next frame
    *previous = mapped;
    *keyboard_prev = *keyboard;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btn_all() {
        let mut gp = Gamepads::default();
        gp.set_data(0xFF);
        assert_eq!(btn(&gp, -1), 0xFF);
    }

    #[test]
    fn btn_single() {
        let mut gp = Gamepads::default();
        gp.set_data(0b00000100);
        assert_eq!(btn(&gp, 2), 4);
        assert_eq!(btn(&gp, 0), 0);
    }

    #[test]
    fn btnp_negative_index() {
        let mut now = Gamepads::default();
        now.set_data(0xFF);
        let prev = Gamepads::default();
        let holds = [0u32; 32];

        // All buttons "just pressed" since prev=0, now=FF
        let result = btnp(&now, &prev, &holds, -1, 0, 0);
        assert_eq!(result, 0xFF, "all buttons should be 'just pressed'");
    }

    #[test]
    fn btnp_positive_index() {
        let mut now = Gamepads::default();
        now.set_data(0b00001000);
        let prev = Gamepads::default();
        let holds = [0u32; 32];

        let result = btnp(&now, &prev, &holds, 3, -1, -1);
        assert_eq!(result, 8);
    }

    #[test]
    fn btnp_hold_repeat() {
        let mut now = Gamepads::default();
        now.set_data(0b00000001);
        let mut prev = Gamepads::default();
        prev.set_data(0b00000001);
        let mut holds = [0u32; 32];
        holds[0] = 5; // held for 5 frames

        // hold=3, period=2 → holds[0]=5 >= 3, holds[0]%2=1 != 0 → use prev
        // prev has bit 0 set, so !prev & now = 0
        let result = btnp(&now, &prev, &holds, 0, 3, 2);
        assert_eq!(result, 0, "repeat not due yet");

        holds[0] = 6; // 6 % 2 = 0 → repeat fires
        let result = btnp(&now, &prev, &holds, 0, 3, 2);
        assert_eq!(result, 1, "repeat should fire");
    }

    #[test]
    fn key_specific() {
        let mut kb = Keyboard::default();
        kb.keys[0] = 42;
        assert!(key(&kb, 42));
        assert!(!key(&kb, 99));
    }

    #[test]
    fn key_any() {
        let mut kb = Keyboard::default();
        kb.data = 1;
        assert!(key(&kb, 0)); // key=0 means "any key"
    }

    #[test]
    fn keyp_fresh_press() {
        let now = Keyboard::default();
        let mut prev = Keyboard::default();
        prev.keys[0] = 42; // was pressed
        let holds = [0u32; 256];
        assert!(!keyp(&now, &prev, &holds, 42, -1, -1)); // no longer down
    }

    #[test]
    fn mouse_absolute() {
        let mut input = Input::default();
        input.mouse.x = 50;
        input.mouse.y = 80;
        // btns: relative=0 (absolute mode)
        let pt = mouse(&input);
        assert_eq!(pt.x, 50 - TIC80_OFFSET_LEFT);
        assert_eq!(pt.y, 80 - TIC80_OFFSET_TOP);
    }

    #[test]
    fn mouse_relative() {
        let mut input = Input::default();
        input.mouse.rx = 10;
        input.mouse.ry = -5;
        input.mouse.btns = 1 << 15; // relative flag
        let pt = mouse(&input);
        assert_eq!(pt.x, 10);
        assert_eq!(pt.y, -5);
    }

    #[test]
    fn tick_io_updates_holds() {
        let mut now = Gamepads::default();
        now.set_data(0b00000001); // button 0 down

        let mut prev = Gamepads::default();
        let mut holds = [0u32; 32];
        let kb = Keyboard::default();
        let mut kb_prev = Keyboard::default();
        let mut kb_holds = [0u32; tic_keys_count];
        let mapping = Mapping::default();

        tick_io(&now, &mut prev, &mut holds, &kb, &mut kb_prev, &mut kb_holds, &mapping);

        // First frame: new press, hold not yet counted
        assert_eq!(holds[0], 0, "first frame new press → hold=0");
        assert_eq!(prev.data(), now.data(), "prev should be updated");

        // Second frame: still held → hold now increments
        tick_io(&now, &mut prev, &mut holds, &kb, &mut kb_prev, &mut kb_holds, &mapping);
        assert_eq!(holds[0], 1, "second frame still held → hold=1");
    }
}
