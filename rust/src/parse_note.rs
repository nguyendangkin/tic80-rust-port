//! Parse a 3-character note string (e.g. `C#4`) into note index + octave.
//!
//! Port of TIC-80's `src/api/parse_note.c`.

/// Standard SFX note names.
const SFX_NOTES: &[&str] = &[
    "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
];

/// Parse a 3-character note string.
///
/// Format: `<note><accidental><octave>`, e.g. `C#4`, `A-3`, `B-5`.
///
/// Returns `Some((note_index, octave))` on success, or `None` if the
/// string doesn't match the expected format.
///
/// - `note_index`: 0–11 (C, C#, D, D#, E, F, F#, G, G#, A, A#, B)
/// - `octave`: 0–8 (character `'1'`–`'9'`)
pub fn parse_note(note_str: &str) -> Option<(usize, usize)> {
    if note_str.len() != 3 {
        return None;
    }

    let chars: [u8; 3] = [
        note_str.as_bytes()[0],
        note_str.as_bytes()[1],
        note_str.as_bytes()[2],
    ];

    // Match first 2 characters against SFX_NOTES
    let note = SFX_NOTES.iter().position(|&n| {
        n.as_bytes()[0] == chars[0] && n.as_bytes()[1] == chars[1]
    })?;

    // Third character is octave (ASCII '1'..='9')
    let octave = (chars[2] as char).to_digit(10)?;
    if octave < 1 || octave > 9 {
        return None;
    }

    Some((note, octave as usize - 1))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_notes() {
        assert_eq!(parse_note("C-4"), Some((0, 3)));  // C octave 4
        assert_eq!(parse_note("C#4"), Some((1, 3)));  // C# octave 4
        assert_eq!(parse_note("D-3"), Some((2, 2)));  // D octave 3
        assert_eq!(parse_note("D#5"), Some((3, 4)));  // D# octave 5
        assert_eq!(parse_note("E-2"), Some((4, 1)));  // E octave 2
        assert_eq!(parse_note("F-1"), Some((5, 0)));  // F octave 1
        assert_eq!(parse_note("F#6"), Some((6, 5)));  // F# octave 6
        assert_eq!(parse_note("G-7"), Some((7, 6)));  // G octave 7
        assert_eq!(parse_note("G#8"), Some((8, 7)));  // G# octave 8
        assert_eq!(parse_note("A-9"), Some((9, 8)));  // A octave 9
        assert_eq!(parse_note("A#4"), Some((10, 3))); // A# octave 4
        assert_eq!(parse_note("B-4"), Some((11, 3))); // B octave 4
    }

    #[test]
    fn parse_invalid_length() {
        assert_eq!(parse_note("C4"), None);    // too short
        assert_eq!(parse_note("C#42"), None);  // too long
        assert_eq!(parse_note(""), None);       // empty
    }

    #[test]
    fn parse_invalid_note() {
        assert_eq!(parse_note("X-4"), None);   // unknown note
        assert_eq!(parse_note("H#4"), None);   // unknown note
    }

    #[test]
    fn parse_invalid_octave() {
        assert_eq!(parse_note("C-0"), None);   // octave 0 (below range)
        assert_eq!(parse_note("C-A"), None);   // non-digit octave
    }

    #[test]
    fn parse_all_notes_in_octave() {
        for i in 0..SFX_NOTES.len() {
            let note_str = SFX_NOTES[i];
            // Append "4" for octave 4 → 3 chars
            let full = format!("{}4", note_str);
            let result = parse_note(&full);
            assert_eq!(result, Some((i, 3)), "failed for {}", full);
        }
    }

    #[test]
    fn parse_all_octaves_one_note() {
        for octave in 1..=9u32 {
            let full = format!("C#{}", octave);
            let result = parse_note(&full);
            assert_eq!(result, Some((1, octave as usize - 1)), "failed for {}", full);
        }
    }
}
