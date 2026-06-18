//! Script language plugin registry.
//!
//! Port of TIC-80's `src/script.c` + `src/script.h`.
//!
//! Manages the list of supported scripting languages (Lua, JS, Ruby, Wren,
//! Fennel, Janet, Squirrel, Scheme, MoonScript, Python, WASM, etc.).
//! Each language is described by a [`Script`] struct containing callback
//! function pointers, syntax rules, and keyword lists.

use crate::tools;

/// Opaque TIC-80 memory handle (defined in C).
#[repr(C)]
pub struct Mem {
    _private: [u8; 0],
}

extern "C" {
    fn tic_cart_get_lang(memory: *const Mem) -> u8;
    fn tic_cart_get_code(memory: *const Mem) -> *const u8;
}

// ---------------------------------------------------------------------------
// Callback function pointer types
// ---------------------------------------------------------------------------

/// Frame tick callback.
pub type TickFn = unsafe extern "C" fn(*mut Mem);
/// Boot callback.
pub type BootFn = unsafe extern "C" fn(*mut Mem);
/// Scanline callback (not in script struct but related).
pub type ScanlineFn = unsafe extern "C" fn(*mut Mem, i32, *mut std::ffi::c_void);
/// Border callback.
pub type BorderFn = unsafe extern "C" fn(*mut Mem, i32, *mut std::ffi::c_void);
/// Game menu callback.
pub type GameMenuFn = unsafe extern "C" fn(*mut Mem, i32, *mut std::ffi::c_void);
/// Blit callback.
// SAFETY: TIC-80 is single-threaded; raw pointers in BlitCallback
// are accessed from one thread only (matching C semantics).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct BlitCallback {
    pub callback: Option<unsafe extern "C" fn(*mut Mem)>,
    pub scanline: Option<ScanlineFn>,
    pub border: Option<BorderFn>,
    pub menu: Option<GameMenuFn>,
    pub data: *mut std::ffi::c_void,
}

/// Outline item for code navigation.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct OutlineItem {
    pub pos: *const std::ffi::c_char,
    pub size: i32,
}

/// Character classifier.
pub type LangIsAlnumFn = unsafe extern "C" fn(c: u8) -> bool;

// ---------------------------------------------------------------------------
// Script descriptor
// ---------------------------------------------------------------------------

// SAFETY: TIC-80 is single-threaded; raw pointers in these types
// are accessed from only one thread.
unsafe impl Sync for BlitCallback {}
unsafe impl Send for BlitCallback {}
unsafe impl Sync for Script {}
unsafe impl Send for Script {}

/// Describes one scripting language that TIC-80 supports.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Script {
    pub id: u8,
    pub name: &'static str,
    pub file_extension: &'static str,
    pub project_comment: &'static str,

    // Callbacks
    pub init: Option<unsafe extern "C" fn(*mut Mem, *const std::ffi::c_char) -> bool>,
    pub close: Option<unsafe extern "C" fn(*mut Mem)>,
    pub tick: Option<TickFn>,
    pub boot: Option<BootFn>,
    pub callback: BlitCallback,

    // Code navigation
    pub get_outline: Option<unsafe extern "C" fn(*const std::ffi::c_char, *mut i32) -> *const OutlineItem>,
    pub eval: Option<unsafe extern "C" fn(*mut Mem, *const std::ffi::c_char)>,

    // Syntax delimiters (None means "not supported")
    pub block_comment_start: Option<&'static str>,
    pub block_comment_end: Option<&'static str>,
    pub block_comment_start2: Option<&'static str>,
    pub block_comment_end2: Option<&'static str>,
    pub block_string_start: Option<&'static str>,
    pub block_string_end: Option<&'static str>,
    pub std_string_start_end: Option<&'static str>,
    /// Single-line comment marker used for `-- script:` meta-tag matching.
    pub single_comment: Option<&'static str>,
    pub block_end: Option<&'static str>,

    // Keywords
    pub keywords: &'static [&'static str],
    pub api_keywords: &'static [&'static str],

    // Character classifier
    pub lang_isalnum: Option<LangIsAlnumFn>,

    // Flags
    pub use_structured_edition: bool,
    pub use_binary_section: bool,
}

// ---------------------------------------------------------------------------
// Global registry
// ---------------------------------------------------------------------------

use std::sync::Mutex;

static REGISTRY: Mutex<Vec<&'static Script>> = Mutex::new(Vec::new());

/// Maximum number of supported languages.
pub const MAX_LANGS: usize = 16;

/// Register a script language.
///
/// Duplicates (by id or name) are silently ignored.
/// Scripts are kept sorted by `id` for deterministic iteration.
pub fn add_script(script: &'static Script) {
    let mut reg = REGISTRY.lock().unwrap();

    // Check for duplicates
    for &s in reg.iter() {
        if s.id == script.id || s.name == script.name {
            return;
        }
    }

    if reg.len() >= MAX_LANGS {
        return;
    }

    reg.push(script);
    reg.sort_by_key(|s| s.id);
}

/// Get the list of all registered scripts.
pub fn scripts() -> Vec<&'static Script> {
    REGISTRY.lock().unwrap().clone()
}

/// Iterate over registered scripts.
///
/// Usage:
/// ```ignore
/// for script in foreach_lang() {
///     // use script
/// }
/// ```
pub fn foreach_lang() -> impl Iterator<Item = &'static Script> {
    scripts().into_iter()
}

/// Find the script engine for a given cartridge.
///
/// First checks `cart.lang` id, then falls back to meta-tag matching
/// (e.g. `-- script: lua`).
pub fn get_script(memory: *const Mem) -> Option<&'static Script> {
    let reg = REGISTRY.lock().unwrap();

    // Try matching by language id
    let lang_id = unsafe { tic_cart_get_lang(memory) };
    for &s in reg.iter() {
        if s.id == lang_id {
            return Some(s);
        }
    }

    // Try matching by meta-tag
    let code_ptr = unsafe { tic_cart_get_code(memory) };
    if !code_ptr.is_null() {
        let code = unsafe { std::ffi::CStr::from_ptr(code_ptr as *const std::ffi::c_char) }
            .to_string_lossy();
        for &s in reg.iter() {
            let tag = tools::metatag(&code, "script", s.single_comment);
            if !tag.is_empty() && tag == s.name {
                return Some(s);
            }
        }
    }

    // Fallback: first registered script
    reg.first().copied()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal script descriptor for testing.
    fn make_test_script(id: u8, name: &'static str) -> &'static Script {
        Box::leak(Box::new(Script {
            id,
            name,
            file_extension: "",
            project_comment: "",
            init: None,
            close: None,
            tick: None,
            boot: None,
            callback: BlitCallback {
                callback: None,
                scanline: None,
                border: None,
                menu: None,
                data: std::ptr::null_mut(),
            },
            get_outline: None,
            eval: None,
            block_comment_start: None,
            block_comment_end: None,
            block_comment_start2: None,
            block_comment_end2: None,
            block_string_start: None,
            block_string_end: None,
            std_string_start_end: Some("\""),
            single_comment: Some("--"),
            block_end: None,
            keywords: &[],
            api_keywords: &[],
            lang_isalnum: None,
            use_structured_edition: false,
            use_binary_section: false,
        }))
    }

    #[test]
    fn add_and_list() {
        // Reset
        *REGISTRY.lock().unwrap() = Vec::new();

        let lua = make_test_script(1, "Lua");
        let js = make_test_script(2, "JavaScript");

        add_script(lua);
        add_script(js);

        let list = scripts();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Lua");
        assert_eq!(list[1].name, "JavaScript");
    }

    #[test]
    fn duplicate_by_id() {
        *REGISTRY.lock().unwrap() = Vec::new();

        let s1 = make_test_script(1, "Lua");
        let s2 = make_test_script(1, "Lua2");

        add_script(s1);
        add_script(s2); // same id → ignored

        assert_eq!(scripts().len(), 1);
    }

    #[test]
    fn duplicate_by_name() {
        *REGISTRY.lock().unwrap() = Vec::new();

        let s1 = make_test_script(1, "Lua");
        let s2 = make_test_script(2, "Lua"); // same name → ignored

        add_script(s1);
        add_script(s2);

        assert_eq!(scripts().len(), 1);
    }

    #[test]
    fn sorted_by_id() {
        *REGISTRY.lock().unwrap() = Vec::new();

        let a = make_test_script(3, "C");
        let b = make_test_script(1, "A");
        let c = make_test_script(2, "B");

        add_script(a);
        add_script(b);
        add_script(c);

        let list = scripts();
        assert_eq!(list[0].name, "A");
        assert_eq!(list[1].name, "B");
        assert_eq!(list[2].name, "C");
    }

    #[test]
    fn foreach_iteration() {
        *REGISTRY.lock().unwrap() = Vec::new();

        add_script(make_test_script(1, "Lua"));
        add_script(make_test_script(2, "JS"));

        let mut count = 0;
        for _s in foreach_lang() {
            count += 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn max_langs() {
        *REGISTRY.lock().unwrap() = Vec::new();

        for i in 0..MAX_LANGS as u8 {
            let name = Box::leak(format!("Lang{}", i).into_boxed_str());
            add_script(make_test_script(i, name));
        }
        // Try adding one more
        add_script(make_test_script(MAX_LANGS as u8, "Overflow"));

        assert_eq!(scripts().len(), MAX_LANGS);
    }
}
