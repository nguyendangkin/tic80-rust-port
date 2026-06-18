//! TIC-80 Rust Port — library crate
//!
//! All ported modules are re-exported here.

pub mod zip;
pub mod json;
pub mod tilesheet;
pub mod tools;
pub mod script;
pub mod cart;
pub mod parse_note;
pub mod io;
pub mod core;
pub mod draw;
pub mod sound;

/// MD5 hasher (ported from `src/ext/md5.c`).
#[path = "../../src/ext/md5.rs"]
pub mod md5;

/// Undo/redo history (ported from `src/ext/history.c`).
#[path = "../../src/ext/history.rs"]
pub mod history;

/// Complex FFT (ported from `src/ext/kiss_fft.c`).
#[path = "../../src/ext/kiss_fft.rs"]
pub mod kiss_fft;

/// Real FFT (ported from `src/ext/kiss_fftr.c`).
#[path = "../../src/ext/kiss_fftr.rs"]
pub mod kiss_fftr;

/// FFT global state + debug logging (ported from `src/fftdata.c`).
#[path = "../../src/fftdata.rs"]
pub mod fftdata;
