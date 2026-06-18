//! Studio subsystem — all phase 1-4 modules in one file.
//!
//! Ports: config.c, fs.c, net.c, project.c, screens/{run,start,console,
//! console_minimal,menu,mainmenu,surf}.c, editors/{code,sprite,map,
//! sfx,music,world}.c
//!
//! This is a structural port with type definitions mirroring the C
//! layout.  Many functions are stubs awaiting the full Rust runtime.

use crate::cart::Cartridge;
use crate::json;
use crate::tools;
use std::ffi::{CStr, CString};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TICNAME_MAX: usize = 256;
pub const CONFIG_TIC_PATH: &str = "config.tic";
pub const TIC_LOCAL: &str = "";
pub const MAX_VOLUME: u8 = 15;
pub const STUDIO_TEXT_BUFFER_WIDTH: usize = 128;
pub const STUDIO_TEXT_BUFFER_SIZE: usize = STUDIO_TEXT_BUFFER_WIDTH * 4;
pub const TIC80_WIDTH: u32 = 240;
pub const TIC80_HEIGHT: u32 = 136;
pub const TIC_SPRITESIZE: u32 = 8;
pub const TIC_MAP_WIDTH: u32 = 30;
pub const TIC_MAP_HEIGHT: u32 = 30;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorMode {
    Start = 0,
    Console = 1,
    Code = 2,
    Sprite = 3,
    Map = 4,
    Music = 5,
    Sfx = 6,
    World = 7,
    Menu = 8,
    Surf = 9,
    Run = 10,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left = 0,
    Middle = 1,
    Right = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorType {
    Arrow = 0,
    Hand = 1,
    Text = 2,
    Cross = 3,
    Wait = 4,
    SizeAll = 5,
    SizeNs = 6,
    SizeWe = 7,
}

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Rgb { pub r: u8, pub g: u8, pub b: u8 }

#[derive(Clone, Debug)]
pub struct CodeTheme {
    pub bg: u8, pub fg: u8, pub string: u8, pub number: u8,
    pub keyword: u8, pub api: u8, pub comment: u8, pub sign: u8,
    pub select: u8, pub cursor: u8,
    pub shadow: bool, pub alt_font: bool, pub alt_caret: bool,
    pub match_delimiters: bool, pub auto_delimiters: bool,
}

impl Default for CodeTheme {
    fn default() -> Self {
        CodeTheme {
            bg: 0, fg: 15, string: 12, number: 6,
            keyword: 10, api: 9, comment: 5, sign: 8,
            select: 0, cursor: 0,
            shadow: true, alt_font: false, alt_caret: false,
            match_delimiters: true, auto_delimiters: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StudioConfig {
    pub theme: ThemeConfig,
    pub check_new_version: bool,
    pub cli: bool,
    pub soft: bool,
    pub trim: bool,
    pub cart: *mut std::ffi::c_void,
    pub ui_scale: i32,
    pub fft: i32,
    pub fft_capture_playback: bool,
    pub fft_device: [u8; 128],
    pub keyboard_layout: i32,
}

impl Default for StudioConfig {
    fn default() -> Self {
        StudioConfig {
            theme: ThemeConfig::default(),
            check_new_version: true, cli: false, soft: false, trim: true,
            cart: std::ptr::null_mut(),
            ui_scale: 4,
            fft: 0, fft_capture_playback: false,
            fft_device: [0u8; 128],
            keyboard_layout: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ThemeConfig {
    pub code: CodeTheme,
    pub gamepad_touch_alpha: u8,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig { code: CodeTheme::default(), gamepad_touch_alpha: 128 }
    }
}

#[derive(Clone, Debug)]
pub struct Point { pub x: i32, pub y: i32 }

#[derive(Clone, Debug)]
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

// ---------------------------------------------------------------------------
// Filesystem (fs.c)
// ---------------------------------------------------------------------------

pub struct Fs {
    pub dir: String,
    pub work: String,
    pub net: *mut std::ffi::c_void,
}

impl Fs {
    pub fn new(path: &str) -> Self {
        Fs { dir: path.to_string(), work: String::new(), net: std::ptr::null_mut() }
    }
    pub fn path(&self, name: &str) -> String { format!("{}/{}", self.dir, name) }
    pub fn path_root(&self, name: &str) -> String { format!("{}/{}", self.dir, name) }
    pub fn load(&self, _name: &str, _size: &mut i32) -> Option<Vec<u8>> { None }
    pub fn load_root(&self, _name: &str, _size: &mut i32) -> Option<Vec<u8>> { None }
    pub fn save(&self, _name: &str, _data: &[u8], _overwrite: bool) -> bool { false }
    pub fn save_root(&self, _name: &str, _data: &[u8], _overwrite: bool) -> bool { false }
    pub fn exists(&self, _name: &str) -> bool { false }
    pub fn is_dir(&self, _name: &str) -> bool { false }
    pub fn del_file(&self, _name: &str) -> bool { false }
    pub fn del_dir(&self, _name: &str) -> bool { false }
    pub fn make_dir(&self, _name: &str) -> bool { false }
    pub fn open_folder(&self) {}
    pub fn is_root(&self) -> bool { true }
    pub fn is_pub_dir(&self) -> bool { false }
    pub fn change_dir(&mut self, dir: &str) { self.work = dir.to_string(); }
    pub fn dir_back(&mut self) {}
    pub fn home_dir(&mut self) { self.work.clear(); }
    pub fn current_dir(&self) -> &str { &self.work }
}

// ---------------------------------------------------------------------------
// Config (config.c)
// ---------------------------------------------------------------------------

pub fn read_config(config: &mut StudioConfig, json_str: &str) {
    let doc = match json::JsonDoc::parse(json_str) { Some(d) => d, None => return };
    let root = doc.root();
    config.check_new_version = json::json_bool("CHECK_NEW_VERSION", root);
    config.ui_scale = json::json_int("UI_SCALE", root);
    config.soft = json::json_bool("SOFTWARE_RENDERING", root);
    config.trim = json::json_bool("TRIM_ON_SAVE", root);
    if config.ui_scale <= 0 { config.ui_scale = 1; }
    config.theme.gamepad_touch_alpha = json::json_int("GAMEPAD_TOUCH_ALPHA", root) as u8;

    if let Some(theme) = json::json_object("CODE_THEME", root) {
        macro_rules! read_color {
            ($field:ident) => {
                config.theme.code.$field = json::json_int(stringify!($field), theme) as u8;
            };
        }
        read_color!(bg); read_color!(fg); read_color!(string);
        read_color!(number); read_color!(keyword); read_color!(api);
        read_color!(comment); read_color!(sign);
        config.theme.code.select = json::json_int("SELECT", theme) as u8;
        config.theme.code.cursor = json::json_int("CURSOR", theme) as u8;
        config.theme.code.shadow = json::json_bool("SHADOW", theme);
        config.theme.code.alt_font = json::json_bool("ALT_FONT", theme);
        config.theme.code.alt_caret = json::json_bool("ALT_CARET", theme);
        config.theme.code.match_delimiters = json::json_bool("MATCH_DELIMITERS", theme);
        config.theme.code.auto_delimiters = json::json_bool("AUTO_DELIMITERS", theme);
    }
}

pub fn set_default_config(config: &mut StudioConfig) {
    *config = StudioConfig::default();
}

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

pub struct Run { pub active: bool }
pub struct Start { pub active: bool, pub stage: i32, pub ticks: i32 }
pub struct Console { pub active: bool }
pub struct ConsoleMinimal { pub active: bool }
pub struct Menu { pub active: bool, pub items: Vec<MenuItem> }
pub struct MenuItem { pub label: String, pub action: i32 }
pub struct MainMenu { pub active: bool }
pub struct Surf { pub active: bool }

// ---------------------------------------------------------------------------
// Editors
// ---------------------------------------------------------------------------

pub struct Code { pub active: bool, pub modified: bool }
pub struct Sprite { pub active: bool, pub selected_index: u16 }
pub struct MapEditor {
    pub active: bool,
    pub scroll: Point,
    pub tiles: Vec<u8>,
}
pub struct SfxEditor { pub active: bool }
pub struct MusicEditor { pub active: bool }
pub struct WorldEditor { pub active: bool }

// ---------------------------------------------------------------------------
// Net
// ---------------------------------------------------------------------------

pub struct Net { pub initialized: bool }

// ---------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------

pub struct Project { pub loaded: bool }

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_read_basic() {
        let json = r#"{"CHECK_NEW_VERSION":true,"UI_SCALE":3,"SOFTWARE_RENDERING":true}"#;
        let mut cfg = StudioConfig::default();
        read_config(&mut cfg, json);
        assert!(cfg.check_new_version);
        assert_eq!(cfg.ui_scale, 3);
        assert!(cfg.soft);
    }

    #[test]
    fn config_read_code_theme() {
        let json = r#"{"CODE_THEME":{"bg":1,"fg":2,"keyword":3,"SELECT":4,"SHADOW":true}}"#;
        let mut cfg = StudioConfig::default();
        read_config(&mut cfg, json);
        assert_eq!(cfg.theme.code.bg, 1);
        assert_eq!(cfg.theme.code.fg, 2);
        assert_eq!(cfg.theme.code.keyword, 3);
        assert_eq!(cfg.theme.code.select, 4);
        assert!(cfg.theme.code.shadow);
    }

    #[test]
    fn config_default_scale() {
        let mut cfg = StudioConfig::default();
        assert_eq!(cfg.ui_scale, 4);
    }

    #[test]
    fn config_read_min_scale() {
        let json = r#"{"UI_SCALE":0}"#;
        let mut cfg = StudioConfig::default();
        read_config(&mut cfg, json);
        assert_eq!(cfg.ui_scale, 1); // clamped
    }

    #[test]
    fn fs_paths() {
        let fs = Fs::new("/home/user/.tic80");
        assert_eq!(fs.path("test.tic"), "/home/user/.tic80/test.tic");
    }

    #[test]
    fn code_theme_defaults() {
        let t = CodeTheme::default();
        assert_eq!(t.fg, 15);
        assert_eq!(t.keyword, 10);
        assert!(t.match_delimiters);
    }

    #[test]
    fn editor_modes() {
        assert_eq!(EditorMode::Start as i32, 0);
        assert_eq!(EditorMode::Code as i32, 2);
        assert_eq!(EditorMode::Sprite as i32, 3);
        assert_eq!(EditorMode::Run as i32, 10);
    }

    #[test]
    fn point_default() {
        let p = Point { x: 10, y: 20 };
        assert_eq!(p.x, 10);
        assert_eq!(p.y, 20);
    }

    #[test]
    fn read_config_invalid_json() {
        let mut cfg = StudioConfig::default();
        read_config(&mut cfg, "not valid json");
        // Should not panic, values stay default
        assert!(cfg.check_new_version);
        assert_eq!(cfg.ui_scale, 4);
    }
}
