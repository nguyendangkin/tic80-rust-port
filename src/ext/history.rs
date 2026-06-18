//! Undo/redo history using XOR-compact diffs.
//!
//! Port of TIC-80's `src/ext/history.c`.
//!
//! Tracks changes to an **external** byte buffer (`data`) by storing XOR
//! differences against a shadow `state` copy.  When the caller modifies
//! `data` externally, calling `add()` captures the diff.  `undo()` /
//! `redo()` replay stored diffs to restore old buffer contents.

// ---------------------------------------------------------------------------
// Diff helpers (pure functions on slices — safe)
// ---------------------------------------------------------------------------

/// Position of the first non-zero byte (or `data.len()` if all zero).
fn trim_left(data: &[u8]) -> usize {
    data.iter().position(|&b| b != 0).unwrap_or(data.len())
}

/// One past the last non-zero byte (or 0 if all zero).
fn trim_right(data: &[u8]) -> usize {
    data.iter()
        .rposition(|&b| b != 0)
        .map_or(0, |i| i + 1)
}

/// XOR `data.buffer` into `state[data.start .. data.end]`.
fn apply_diff(state: &mut [u8], diff: &Diff) {
    let range = diff.start..diff.end;
    for (s, &b) in state[range].iter_mut().zip(diff.buffer.iter()) {
        *s ^= b;
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A compact XOR-diff between two snapshots.
struct Diff {
    buffer: Vec<u8>,
    start: usize,
    end: usize,
}

/// Undo/redo history.
///
/// `data` is a **raw pointer** to an external buffer the caller manages.
/// History never allocates or frees `data` — it only reads and writes bytes.
///
/// # Safety
///
/// The pointer passed to [`new`](History::new) must point to a valid,
/// non-aliased buffer of at least `size` bytes for the entire lifetime of
/// the `History`.  All public methods are `unsafe` because they dereference
/// this pointer.
pub struct History {
    data: *mut u8,
    size: usize,
    /// Shadow copy of `data` — always reflects the last snapshot.
    state: Vec<u8>,
    /// Diffs in chronological order.  Entry 0 is always an empty diff
    /// representing the initial state.
    entries: Vec<Diff>,
    /// Index of the "current" entry (the state `data` is currently at).
    current: usize,
}

// The raw pointer kills auto-Send/Sync — matching the (single-threaded)
// design of the original C code.
//
// `*mut u8` is `!Send + !Sync` by default in Rust, which is correct:
// History must not be sent across threads or shared.

impl History {
    /// Create a new History tracking the buffer at `data` of `size` bytes.
    ///
    /// An initial snapshot is taken immediately.
    ///
    /// # Safety
    ///
    /// - `data` must point to a valid buffer of **at least `size` bytes**.
    /// - The buffer must remain valid, non-aliased, and **not reallocated**
    ///   for the lifetime of this `History`.
    /// - No concurrent reads/writes to `data` while any `History` method
    ///   is active.
    pub unsafe fn new(data: *mut u8, size: usize) -> Self {
        let mut state = vec![0u8; size];
        std::ptr::copy_nonoverlapping(data, state.as_mut_ptr(), size);

        History {
            data,
            size,
            state,
            entries: vec![Diff { buffer: Vec::new(), start: 0, end: 0 }],
            current: 0,
        }
    }

    /// Record a new history entry if the data has changed.
    ///
    /// Returns `true` if a new entry was added, `false` if the buffer is
    /// unchanged since the last snapshot.
    ///
    /// Diffs are stored compactly: leading and trailing zero bytes are
    /// trimmed away.  If the current position is not at the tip of the
    /// history (i.e. the user has undone some changes), any forward
    /// entries are discarded — new history replaces the redo branch.
    ///
    /// # Safety
    ///
    /// Reads the tracked buffer via the internal raw pointer.
    pub unsafe fn add(&mut self) -> bool {
        let data =
            std::slice::from_raw_parts(self.data, self.size);

        // Fast path: nothing changed
        if self.state.as_slice() == data {
            return false;
        }

        // state ^= data  →  state now holds the bytewise XOR diff
        for i in 0..self.size {
            self.state[i] ^= data[i];
        }

        let start = trim_left(&self.state);
        let end = trim_right(&self.state);

        debug_assert!(
            start < end,
            "state != data but XOR diff is all-zero — impossible"
        );

        // Store the compact diff
        let len = end - start;
        let mut buffer = vec![0u8; len];
        std::ptr::copy_nonoverlapping(
            self.state.as_ptr().add(start),
            buffer.as_mut_ptr(),
            len,
        );

        // Discard redo entries beyond current position
        self.entries.truncate(self.current + 1);
        self.entries.push(Diff { buffer, start, end });
        self.current = self.entries.len() - 1;

        // Restore state = current data
        self.state.copy_from_slice(data);

        true
    }

    /// Undo the most recent change.
    ///
    /// No-op if there is nothing to undo (already at the initial state).
    ///
    /// # Safety
    ///
    /// Reads and writes the tracked buffer via the internal raw pointer.
    pub unsafe fn undo(&mut self) {
        if self.current > 0 {
            apply_diff(&mut self.state, &self.entries[self.current]);
            self.current -= 1;
        }
        std::ptr::copy_nonoverlapping(
            self.state.as_ptr(),
            self.data,
            self.size,
        );
    }

    /// Redo the previously undone change.
    ///
    /// No-op if there is nothing to redo (already at the latest state).
    ///
    /// # Safety
    ///
    /// Reads and writes the tracked buffer via the internal raw pointer.
    pub unsafe fn redo(&mut self) {
        if self.current + 1 < self.entries.len() {
            self.current += 1;
            apply_diff(&mut self.state, &self.entries[self.current]);
        }
        std::ptr::copy_nonoverlapping(
            self.state.as_ptr(),
            self.data,
            self.size,
        );
    }

    // ---- query helpers (useful for testing / integration) ----

    /// Number of stored entries (including the initial empty entry).
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Index of the current entry.
    pub fn current_index(&self) -> usize {
        self.current
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a mutable buffer and a History watching it.
    unsafe fn setup(data: &mut [u8]) -> History {
        History::new(data.as_mut_ptr(), data.len())
    }

    // ---------- basic flow ----------

    #[test]
    fn add_undo_redo() {
        let mut buf = vec![0u8; 16];
        let mut h = unsafe { setup(&mut buf) };

        // Modify the buffer directly (simulating an editor action)
        buf[5] = 0xAB;
        buf[10] = 0xCD;
        assert!(unsafe { h.add() }, "add should report change");

        // Undo
        unsafe { h.undo() };
        assert_eq!(buf, vec![0u8; 16], "undo should restore zeros");

        // Redo
        unsafe { h.redo() };
        let mut expected = vec![0u8; 16];
        expected[5] = 0xAB;
        expected[10] = 0xCD;
        assert_eq!(buf, expected, "redo should re-apply changes");
    }

    // ---------- multiple steps ----------

    #[test]
    fn multiple_undo_redo() {
        let mut buf = vec![0u8; 8];
        let mut h = unsafe { setup(&mut buf) };

        // Change 1
        buf[0] = 1;
        assert!(unsafe { h.add() });

        // Change 2
        buf[1] = 2;
        assert!(unsafe { h.add() });

        // Change 3
        buf[2] = 3;
        assert!(unsafe { h.add() });

        // Undo all three
        unsafe { h.undo() };
        assert_eq!(buf[2], 0, "after undo-1: byte 2 should be 0");
        assert_eq!(buf[1], 2, "after undo-1: byte 1 should still be 2");
        unsafe { h.undo() };
        assert_eq!(buf[1], 0, "after undo-2: byte 1 should be 0");
        assert_eq!(buf[0], 1, "after undo-2: byte 0 should still be 1");
        unsafe { h.undo() };
        assert_eq!(buf, vec![0u8; 8], "after undo-3: all zeros");

        // Redo all three
        unsafe { h.redo() };
        assert_eq!(buf[0], 1, "after redo-1");
        unsafe { h.redo() };
        assert_eq!(buf[1], 2, "after redo-2");
        unsafe { h.redo() };
        assert_eq!(buf[2], 3, "after redo-3");

        // Extra redo should be a no-op
        unsafe { h.redo() };
        assert_eq!(buf[2], 3, "extra redo should not change anything");
    }

    // ---------- branching (add after undo discards redo) ----------

    #[test]
    fn branching_undo_discards_redo() {
        let mut buf = vec![0u8; 4];
        let mut h = unsafe { setup(&mut buf) };

        buf[0] = 1;
        unsafe { h.add(); }
        buf[0] = 2;
        unsafe { h.add(); }

        // Undo back to state 1
        unsafe { h.undo() };
        assert_eq!(buf[0], 1, "after undo: state 1");

        // Now make a new change (branch)
        buf[0] = 99;
        unsafe { h.add(); }

        // Redo should NOT go back to state 2 (it was discarded)
        unsafe { h.undo() };
        assert_eq!(buf[0], 1, "undo branch: back to state 1");
        unsafe { h.redo() };
        assert_eq!(buf[0], 99, "redo branch: state 99 (not 2)");

        // The old redo path (state 2) is lost
        assert_eq!(h.entry_count(), 3, "only initial + 2 branch entries");
    }

    // ---------- no-op add ----------

    #[test]
    fn add_no_change() {
        let mut buf = vec![0x42u8; 4];
        let mut h = unsafe { setup(&mut buf) };

        // Don't change anything
        assert!(!unsafe { h.add() }, "add with no change should return false");
        assert_eq!(h.entry_count(), 1, "no new entry added");
    }

    // ---------- trimming ----------

    #[test]
    fn trimmed_diff() {
        let mut buf = vec![0u8; 64];
        let mut h = unsafe { setup(&mut buf) };

        // Change a single byte in the middle
        buf[30] = 0xFF;
        unsafe { h.add(); }

        // Undo should only affect byte 30
        unsafe { h.undo() };
        assert_eq!(buf[30], 0, "byte 30 restored");
        assert_eq!(buf[0], 0, "byte 0 untouched");

        // Redo should only affect byte 30
        unsafe { h.redo() };
        assert_eq!(buf[30], 0xFF, "byte 30 changed back");
        assert_eq!(buf[0], 0, "byte 0 still untouched");
    }

    // ---------- single byte buffer ----------

    #[test]
    fn single_byte() {
        let mut buf = vec![0u8; 1];
        let mut h = unsafe { setup(&mut buf) };

        buf[0] = 7;
        unsafe { h.add(); }
        assert_eq!(buf[0], 7);

        unsafe { h.undo(); }
        assert_eq!(buf[0], 0);

        unsafe { h.redo(); }
        assert_eq!(buf[0], 7);
    }

    // ---------- large buffer ----------

    #[test]
    fn large_buffer() {
        let mut buf = vec![0u8; 1024];
        let mut h = unsafe { setup(&mut buf) };

        // Write a pattern
        for i in 0..1024 {
            buf[i] = (i & 0xFF) as u8;
        }
        unsafe { h.add(); }

        // Modify near the end
        buf[900] = 0x42;
        buf[901] = 0x43;
        unsafe { h.add(); }

        // Undo back to pattern
        unsafe { h.undo(); }
        assert_eq!(buf[900], (900 & 0xFF) as u8, "undo restored byte 900");
        assert_eq!(buf[901], (901 & 0xFF) as u8, "undo restored byte 901");

        // Undo back to zeros
        unsafe { h.undo(); }
        assert_eq!(buf[0], 0, "initial state restored");
    }

    // ---------- undo at boundary (no-op) ----------

    #[test]
    fn undo_when_at_start() {
        let mut buf = vec![1u8; 8];
        let mut h = unsafe { setup(&mut buf) };

        // At initial state — undo should be a no-op
        unsafe { h.undo() };
        assert_eq!(buf, vec![1u8; 8], "undo at start should be no-op");

        // Make a change and undo back
        buf[0] = 0;
        unsafe { h.add(); }
        unsafe { h.undo(); }
        assert_eq!(buf, vec![1u8; 8], "undo applied");

        // Undo again — should be no-op
        unsafe { h.undo() };
        assert_eq!(buf, vec![1u8; 8], "second undo at start should be no-op");
    }

    // ---------- redo at boundary (no-op) ----------

    #[test]
    fn redo_when_at_end() {
        let mut buf = vec![0u8; 4];
        let mut h = unsafe { setup(&mut buf) };

        // Make a change
        buf[0] = 42;
        unsafe { h.add(); }

        // Redo at tip should be a no-op
        unsafe { h.redo() };
        assert_eq!(buf[0], 42, "redo at tip should be no-op");
    }

    // ---------- zero-sized buffer ----------

    #[test]
    fn zero_size() {
        let mut buf: [u8; 0] = [];
        let mut h = unsafe { History::new(buf.as_mut_ptr(), 0) };

        // add on zero-sized buffer — nothing can change
        assert!(!unsafe { h.add() }, "no bytes to change");
        unsafe { h.undo() };
        unsafe { h.redo() };
        // All no-ops, no crash  →  pass
    }
}
