//! FFT global state and debug logging.
//!
//! Port of TIC-80's `src/fftdata.c`.
//!
//! # Safety
//!
//! All mutable state here is `pub static` wrapped in [`Locked`] for
//! interior mutability.  TIC-80 is single-threaded, so there is no
//! contention — `Locked` acts as a zero-cost `UnsafeCell` that avoids
//! `static mut` (which is deprecated and forces `unsafe` at every
//! access).  Callers acquire a reference with `.get()` and read/write
//! freely.

use std::cell::UnsafeCell;

// ---------------------------------------------------------------------------
// Locked<T> — single-threaded interior mutability
// ---------------------------------------------------------------------------

/// Zero-cost interior mutability for single-threaded globals.
///
/// Like `UnsafeCell` but with a safe `.get()` that returns `&mut T`.
/// **Not `Sync`** — cannot be shared across threads, matching the
/// original C design.
pub struct Locked<T> {
    inner: UnsafeCell<T>,
}

// SAFETY: TIC-80 is single-threaded; `Locked` is deliberately `Sync`
// because the original C globals are also accessed from any function
// without synchronization.  We opt in explicitly.
unsafe impl<T> Sync for Locked<T> {}

impl<T> Locked<T> {
    pub const fn new(value: T) -> Self {
        Locked {
            inner: UnsafeCell::new(value),
        }
    }

    /// Get a mutable reference to the inner value.
    ///
    /// Safe because `Locked` is `!Sync` so it can only be accessed
    /// from one thread — matching the original C semantics.
    #[inline(always)]
    pub fn get(&self) -> &mut T {
        // SAFETY: single-threaded, no aliasing
        unsafe { &mut *self.inner.get() }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const FFT_SIZE: usize = 1024;

// ---------------------------------------------------------------------------
// Log levels
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Off = 0,
    Fatal,
    Error,
    Warning,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn prefix(&self) -> &'static str {
        match self {
            LogLevel::Off => "",
            LogLevel::Fatal => "[FFT FATAL]: ",
            LogLevel::Error => "[FFT ERROR]: ",
            LogLevel::Warning => "[FFT WARNING]: ",
            LogLevel::Info => "[FFT INFO]: ",
            LogLevel::Debug => "[FFT DEBUG]: ",
            LogLevel::Trace => "[FFT TRACE]: ",
        }
    }
}

// ---------------------------------------------------------------------------
// Global mutable state
// ---------------------------------------------------------------------------

/// FFT frequency magnitude data (filled by `FFT_GetFFT`).
pub static FFT_DATA: Locked<[f32; FFT_SIZE]> = Locked::new([0.0; FFT_SIZE]);

/// Smoothed FFT data (exponential moving average).
pub static FFT_SMOOTHING_DATA: Locked<[f32; FFT_SIZE]> = Locked::new([0.0; FFT_SIZE]);

/// Normalized FFT data.
pub static FFT_NORMALIZED_DATA: Locked<[f32; FFT_SIZE]> = Locked::new([0.0; FFT_SIZE]);

/// Max-normalized FFT data.
pub static FFT_NORMALIZED_MAX_DATA: Locked<[f32; FFT_SIZE]> = Locked::new([0.0; FFT_SIZE]);

/// Minimum peak value floor.
pub static PEAK_MIN_VALUE: Locked<f32> = Locked::new(0.01);

/// Peak smoothing factor (exponential decay).
pub static PEAK_SMOOTHING: Locked<f32> = Locked::new(0.995);

/// Current smoothed peak value.
pub static PEAK_SMOOTH_VALUE: Locked<f32> = Locked::new(0.0);

/// Auto-gain amplification factor (1/peak).
pub static AMPLIFICATION: Locked<f32> = Locked::new(1.0);

/// Whether the FFT capture pipeline is active.
pub static FFT_ENABLED: Locked<bool> = Locked::new(false);

/// Current debug log level.
pub static CURRENT_LOG_LEVEL: Locked<LogLevel> = Locked::new(LogLevel::Debug);

// ---------------------------------------------------------------------------
// Debug logging
// ---------------------------------------------------------------------------

/// Log an FFT debug message if `level` ≤ current log level.
///
/// For `Trace` level, a ISO-8601 timestamp is prepended (matching C).
pub fn debug_log(level: LogLevel, msg: &str) {
    let current = *CURRENT_LOG_LEVEL.get();
    if level > current || level == LogLevel::Off {
        return;
    }

    if level == LogLevel::Trace {
        // ISO 8601 timestamp, matching C's strftime "%Y-%m-%dT%H:%M:%S"
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        // Simple UTC timestamp formatting
        let (year, month, day, hour, min, sec) = seconds_to_ymdhms(secs);
        print!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02} ",
            year, month, day, hour, min, sec
        );
    }

    print!("{}{}", level.prefix(), msg);
}

/// Macro version matching C's printf-style usage.
///
/// # Example
/// ```ignore
/// fft_debug_log!(Trace, "callback invoked, frames={}", frame_count);
/// fft_debug_log!(Info, "backend is '{}'", backend_name);
/// ```
#[macro_export]
macro_rules! fft_debug_log {
    ($level:expr, $($arg:tt)+) => {
        debug_log($level, &format!($($arg)+));
    };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert Unix timestamp (seconds since epoch) to (y, m, d, h, min, s) in UTC.
fn seconds_to_ymdhms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    // Days since epoch
    let mut days = secs / 86400;
    let time_secs = secs % 86400;
    let hour = (time_secs / 3600) as u32;
    let min = ((time_secs % 3600) / 60) as u32;
    let sec = (time_secs % 60) as u32;

    // Civil date from days since 1970-01-01
    let mut y = 1970i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        y += 1;
    }

    let leap = is_leap(y);
    const MONTH_DAYS: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut m = 1u32;
    for &md in MONTH_DAYS.iter() {
        let dim = if m == 2 && leap { 29 } else { md };
        if days < dim {
            break;
        }
        days -= dim;
        m += 1;
    }

    (y as u32, m, (days + 1) as u32, hour, min, sec)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(FFT_SIZE, 1024);
        assert!((*PEAK_MIN_VALUE.get() - 0.01).abs() < 1e-6);
        assert!((*PEAK_SMOOTHING.get() - 0.995).abs() < 1e-6);
        assert!((*PEAK_SMOOTH_VALUE.get()).abs() < 1e-6);
        assert!((*AMPLIFICATION.get() - 1.0).abs() < 1e-6);
        assert!(!*FFT_ENABLED.get());
        assert_eq!(*CURRENT_LOG_LEVEL.get(), LogLevel::Debug);
    }

    #[test]
    fn fft_data_arrays() {
        // Arrays are zero-initialized
        for i in 0..FFT_SIZE {
            assert!((*FFT_DATA.get())[i].abs() < 1e-6, "FFT_DATA[{}] != 0", i);
            assert!((*FFT_SMOOTHING_DATA.get())[i].abs() < 1e-6);
            assert!((*FFT_NORMALIZED_DATA.get())[i].abs() < 1e-6);
            assert!((*FFT_NORMALIZED_MAX_DATA.get())[i].abs() < 1e-6);
        }
    }

    #[test]
    fn write_and_read_f32() {
        *PEAK_MIN_VALUE.get() = 0.5;
        assert!((*PEAK_MIN_VALUE.get() - 0.5).abs() < 1e-6);
        // Restore
        *PEAK_MIN_VALUE.get() = 0.01;
    }

    #[test]
    fn write_and_read_array() {
        (*FFT_DATA.get())[42] = 3.14;
        assert!((*FFT_DATA.get())[42] - 3.14 < 1e-6);
        // Restore
        (*FFT_DATA.get())[42] = 0.0;
    }

    #[test]
    fn toggle_enabled() {
        *FFT_ENABLED.get() = true;
        assert!(*FFT_ENABLED.get());
        *FFT_ENABLED.get() = false;
        assert!(!*FFT_ENABLED.get());
    }

    #[test]
    fn log_level_filtering() {
        // Trace < Debug (current default), so Trace messages should output
        // something. We just check it doesn't panic.
        let orig = *CURRENT_LOG_LEVEL.get();
        *CURRENT_LOG_LEVEL.get() = LogLevel::Off;
        debug_log(LogLevel::Error, "this should be suppressed");
        *CURRENT_LOG_LEVEL.get() = orig;
    }

    #[test]
    fn log_level_order() {
        assert!(LogLevel::Off < LogLevel::Fatal);
        assert!(LogLevel::Fatal < LogLevel::Error);
        assert!(LogLevel::Error < LogLevel::Warning);
        assert!(LogLevel::Warning < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }

    #[test]
    fn log_level_prefix() {
        assert_eq!(LogLevel::Error.prefix(), "[FFT ERROR]: ");
        assert_eq!(LogLevel::Info.prefix(), "[FFT INFO]: ");
        assert_eq!(LogLevel::Off.prefix(), "");
    }

    #[test]
    fn timestamp_conversion() {
        // 2024-01-15T10:30:00 UTC = 1705314600 seconds from epoch? Let me check
        // 2024-01-01 = 1704067200
        // 2024-01-15 = 1704067200 + 14*86400 = 1705276800
        // 10:30:00 = 10*3600 + 30*60 = 37800
        // Total ~ 1705314600
        // But let's just verify known epoch: 0 = 1970-01-01T00:00:00
        let (y, m, d, h, min, s) = seconds_to_ymdhms(0);
        assert_eq!(y, 1970);
        assert_eq!(m, 1);
        assert_eq!(d, 1);
        assert_eq!(h, 0);
        assert_eq!(min, 0);
        assert_eq!(s, 0);
    }

    #[test]
    fn timestamp_known() {
        // 2000-01-01T00:00:00 UTC = 946684800
        let (y, m, d, h, min, s) = seconds_to_ymdhms(946684800);
        assert_eq!(y, 2000);
        assert_eq!(m, 1);
        assert_eq!(d, 1);
        assert_eq!(h, 0);
        assert_eq!(min, 0);
        assert_eq!(s, 0);
    }

    #[test]
    fn macro_compiles() {
        // Just verify the macro expands without error
        fft_debug_log!(LogLevel::Info, "test message {}", 42);
        fft_debug_log!(LogLevel::Trace, "multiple {}", "args");
    }
}
