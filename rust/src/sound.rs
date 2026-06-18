//! Audio engine — SFX, music tracker, sound synthesis.
//!
//! Port of TIC-80's `src/core/sound.c`.

use crate::tools;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const BITS_IN_BYTE: usize = 8;
pub const NOTES_PER_BEAT: u32 = 4;
pub const TIC80_FRAMERATE: u32 = 60;
pub const CLOCKRATE: u32 = 255 << 13;
pub const WAVE_VALUES: usize = 68;
pub const MAX_VOLUME: u32 = 15;
pub const SFX_TICKS: usize = 64;
pub const NOTES: usize = 12;
pub const OCTAVES: usize = 10;
pub const DEFAULT_TEMPO: i32 = 125; // 120 bpm * 10 - 1200?
pub const DEFAULT_SPEED: i32 = 6;
pub const SFX_DEF_SPEED: i32 = 0;
pub const SFX_SPEED_BITS: i32 = 4;
pub const SFX_COUNT_BITS: u32 = 6;
pub const SFX_COUNT: u32 = 1 << SFX_COUNT_BITS;
pub const MUSIC_FRAMES: u32 = 16;
pub const MUSIC_PATTERN_ROWS: u32 = 64;
pub const MUSIC_CMD_BITS: u32 = 3;
pub const PATTERN_START: i32 = 1;
pub const PITCH_DELTA: i32 = 0x80;
pub const SHRT_MAX: i32 = 32767;
pub const UCHAR_MAX: i32 = 255;
pub const TIC_SOUND_CHANNELS: usize = 4;
pub const TIC_SOUND_RINGBUF_LEN: usize = 12;
pub const WAVE_SIZE: usize = 68;
pub const ENVELOPE_FREQ_SCALE: i32 = 2;
pub const SECONDS_PER_MINUTE: u32 = 60;
pub const NOTES_PER_MINUTE: u32 = TIC80_FRAMERATE / NOTES_PER_BEAT * SECONDS_PER_MINUTE;
pub const PIANO_START: u32 = 8;
pub const ENDTIME: u32 = CLOCKRATE / TIC80_FRAMERATE;
pub const TIC_PCM_SIZE: usize = 64;

// ---------------------------------------------------------------------------
// Note frequency table
// ---------------------------------------------------------------------------

pub const NOTE_FREQS: [u16; 104] = [
    0x10, 0x11, 0x12, 0x13, 0x15, 0x16, 0x17, 0x18, 0x1a, 0x1c, 0x1d, 0x1f,
    0x21, 0x23, 0x25, 0x27, 0x29, 0x2c, 0x2e, 0x31, 0x34, 0x37, 0x3a, 0x3e,
    0x41, 0x45, 0x49, 0x4e, 0x52, 0x57, 0x5c, 0x62, 0x68, 0x6e, 0x75, 0x7b,
    0x83, 0x8b, 0x93, 0x9c, 0xa5, 0xaf, 0xb9, 0xc4, 0xd0, 0xdc, 0xe9, 0xf7,
    0x106, 0x115, 0x126, 0x137, 0x14a, 0x15d, 0x172, 0x188, 0x19f, 0x1b8,
    0x1d2, 0x1ee, 0x20b, 0x22a, 0x24b, 0x26e, 0x293, 0x2ba, 0x2e4, 0x310,
    0x33f, 0x370, 0x3a4, 0x3dc, 0x417, 0x455, 0x497, 0x4dd, 0x527, 0x575,
    0x5c8, 0x620, 0x67d, 0x6e0, 0x749, 0x7b8, 0x82d, 0x8a9, 0x92d, 0x9b9,
    0xa4d, 0xaea, 0xb90, 0xc40, 0xcfa, 0xdc0, 0xe91, 0xf6f, 0x105a, 0x1153,
    0x125b, 0x1372, 0x149a, 0x15d4, 0x1720, 0x1880,
];

// ---------------------------------------------------------------------------
// Blip-buf wrappers (stubs for now — link against C blip_buf for full use)
// ---------------------------------------------------------------------------

// These are used by the synthesis pipeline; when linking the full TIC-80
// binary, replace with FFI to the C blip_buf library.

#[cfg(not(test))]
mod blip {
    extern "C" {
        pub fn blip_new(size: i32) -> *mut std::ffi::c_void;
        pub fn blip_delete(blip: *mut std::ffi::c_void);
        pub fn blip_set_rates(blip: *mut std::ffi::c_void, clock_rate: f64, sample_rate: f64);
        pub fn blip_add_delta(blip: *mut std::ffi::c_void, time: i32, delta: i32);
        pub fn blip_end_frame(blip: *mut std::ffi::c_void, time: u32);
        pub fn blip_read_samples(blip: *mut std::ffi::c_void, buffer: *mut i16, count: i32, stereo: i32) -> i32;
        pub fn blip_clear(blip: *mut std::ffi::c_void);
    }
}

#[cfg(test)]
mod blip {
    #[allow(improper_ctypes)]
    pub unsafe fn blip_new(_size: i32) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
    pub unsafe fn blip_delete(_blip: *mut std::ffi::c_void) {}
    pub unsafe fn blip_set_rates(_blip: *mut std::ffi::c_void, _clock: f64, _sample: f64) {}
    pub unsafe fn blip_add_delta(_blip: *mut std::ffi::c_void, _time: i32, _delta: i32) {}
    pub unsafe fn blip_end_frame(_blip: *mut std::ffi::c_void, _time: u32) {}
    pub unsafe fn blip_read_samples(
        _blip: *mut std::ffi::c_void,
        _buffer: *mut i16,
        _count: i32,
        _stereo: i32,
    ) -> i32 { 0 }
    pub unsafe fn blip_clear(_blip: *mut std::ffi::c_void) {}
}

// ---------------------------------------------------------------------------
// Core types used by sound
// ---------------------------------------------------------------------------

/// Track row (matching C bitfield layout, 3 bytes).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TrackRow(pub [u8; 3]);

impl TrackRow {
    pub fn note(&self) -> u8 { self.0[0] & 0x0f }
    pub fn param1(&self) -> u8 { (self.0[0] >> 4) & 0x0f }
    pub fn param2(&self) -> u8 { self.0[1] & 0x0f }
    pub fn command(&self) -> u8 { (self.0[1] >> 4) & 0x07 }
    pub fn sfxhi(&self) -> u8 { (self.0[1] >> 7) & 1 }
    pub fn sfxlow(&self) -> u8 { self.0[2] & 0x1f }
    pub fn octave(&self) -> u8 { (self.0[2] >> 5) & 0x07 }
    
    pub fn sfx(&self) -> i32 {
        ((self.sfxhi() as i32) << 5) | (self.sfxlow() as i32)
    }
}

/// Music command values
pub const CMD_EMPTY: u8 = 0;
pub const CMD_VOLUME: u8 = 1;
pub const CMD_CHORD: u8 = 2;
pub const CMD_JUMP: u8 = 3;
pub const CMD_VIBRATO: u8 = 4;
pub const CMD_SLIDE: u8 = 5;
pub const CMD_PITCH: u8 = 6;
pub const CMD_DELAY: u8 = 7;

/// Sound register data for one channel.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct SoundRegisterData {
    pub time: i32,
    pub phase: i32,
    pub amp: i32,
}

/// Sound register (waveform + volume + freq).
#[derive(Clone, Debug)]
#[repr(C)]
pub struct SoundRegister {
    pub waveform: Waveform,
    pub volume: u8,
    _pad: [u8; 5],
    pub freq: u16,
}

/// PCM data.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Pcm {
    pub data: [u8; TIC_PCM_SIZE],
}

/// Loop point.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SoundLoop {
    pub start: u8,
    pub size: u8,
}

/// Sound effect data (sample).
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Sample {
    pub data: [SampleChannel; SFX_TICKS],
    pub speed: u8,
    pub volume: u8,
    pub pitch16x: u8,
    pub reverse: u8,
    pub stereo_left: u8,
    pub stereo_right: u8,
    pub loops: [SoundLoop; 4],
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SampleChannel {
    pub volume: u8,
    pub chord: i8,
    pub pitch: i8,
    pub wave: u8,
}

/// Waveform (68 bytes).
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Waveform {
    pub data: [u8; WAVE_SIZE],
}

/// Track pattern.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct TrackPattern {
    pub rows: [TrackRow; MUSIC_PATTERN_ROWS as usize],
}

/// Track data.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Track {
    pub data: [u8; (MUSIC_FRAMES as usize) * 3],
    pub tempo: i8,
    pub rows: u8,
    pub speed: i8,
}

/// Track row with parameter value.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TrackRowEx {
    pub row: TrackRow,
    pub param_val: i32,
}

/// Channel data for SFX playback.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct ChannelData {
    pub tick: i32,
    pub pos: [i16; 4], // sfx_pos
    pub index: i32,
    pub note: i32,
    pub volume_left: u8,
    pub volume_right: u8,
    pub speed: i8,
    pub duration: i32,
}

impl Default for ChannelData {
    fn default() -> Self {
        ChannelData {
            tick: -1,
            pos: [-1i16; 4],
            index: -1,
            note: 0,
            volume_left: 0,
            volume_right: 0,
            speed: 0,
            duration: 0,
        }
    }
}

/// Command data for music processing.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct CommandData {
    pub chord_tick: i32,
    pub chord_note1: u8,
    pub chord_note2: u8,
    pub vibrato_tick: i32,
    pub vibrato_period: u8,
    pub vibrato_depth: u8,
    pub slide_tick: i32,
    pub slide_note: i32,
    pub slide_duration: i32,
    pub finepitch_value: i32,
    pub delay_row: Option<TrackRow>,
    pub delay_ticks: i32,
}

impl Default for CommandData {
    fn default() -> Self {
        CommandData {
            chord_tick: 0,
            chord_note1: 0,
            chord_note2: 0,
            vibrato_tick: 0,
            vibrato_period: 0,
            vibrato_depth: 0,
            slide_tick: 0,
            slide_note: 0,
            slide_duration: 0,
            finepitch_value: 0,
            delay_row: None,
            delay_ticks: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

pub fn param2val(row: &TrackRow) -> i32 {
    (row.param1() as i32) << 4 | row.param2() as i32
}

pub fn freq2period(freq: i32) -> i32 {
    const MIN_PERIOD: i32 = 10;
    const MAX_PERIOD: i32 = 4096;
    const RATE: i32 = (CLOCKRATE as i32) * ENVELOPE_FREQ_SCALE / (WAVE_VALUES as i32);

    if freq == 0 { return MAX_PERIOD; }

    CLAMP(RATE / freq - 1, MIN_PERIOD, MAX_PERIOD)
}

fn CLAMP(x: i32, min: i32, max: i32) -> i32 {
    if x < min { min } else if x > max { max } else { x }
}

pub fn get_amp(volume: i32, amp: i32) -> i32 {
    amp * volume / (MAX_VOLUME as i32) / ((TIC_SOUND_CHANNELS as i32) + 1)
}

// ---------------------------------------------------------------------------
// Blip-buf wrappers
// ---------------------------------------------------------------------------

pub fn update_amp(blip: *mut std::ffi::c_void, data: &mut SoundRegisterData, new_amp: i32) {
    let delta = new_amp - data.amp;
    data.amp += delta;
    unsafe { blip::blip_add_delta(blip, data.time, delta); }
}

// ---------------------------------------------------------------------------
// Sound synthesis
// ---------------------------------------------------------------------------

pub fn run_envelope(
    blip: *mut std::ffi::c_void,
    reg: &SoundRegister,
    data: &mut SoundRegisterData,
    stereo_volume: u8,
) {
    let period = freq2period(reg.freq as i32 * ENVELOPE_FREQ_SCALE);

    while data.time < ENDTIME as i32 {
        let wave_val =
            unsafe { crate::tilesheet::peek4(reg.waveform.data.as_ptr(), data.phase as u32) };
        let amp = get_amp(
            reg.volume as i32,
            (wave_val as i32) * SHRT_MAX / (MAX_VOLUME as i32) * (stereo_volume as i32) / (MAX_VOLUME as i32),
        );
        update_amp(blip, data, amp);
        data.time += period;
        data.phase = (data.phase + 1) % (WAVE_VALUES as i32);
    }
}

pub fn run_noise(
    blip: *mut std::ffi::c_void,
    reg: &SoundRegister,
    data: &mut SoundRegisterData,
    stereo_volume: u8,
) {
    if data.phase == 0 { data.phase = 1; }

    let period = freq2period(reg.freq as i32);
    let fb: i32 = if reg.waveform.data[0] != 0 { 0x14 } else { 0x12000 };

    while data.time < ENDTIME as i32 {
        let amp = if (data.phase & 1) != 0 {
            get_amp(
                reg.volume as i32,
                (stereo_volume as i32) * SHRT_MAX / (MAX_VOLUME as i32),
            )
        } else {
            0
        };
        update_amp(blip, data, amp);
        data.time += period;
        data.phase = ((data.phase & 1) * fb) ^ (data.phase >> 1);
    }
}

pub fn run_pcm(
    blip: *mut std::ffi::c_void,
    pcm: &Pcm,
    data: &mut SoundRegisterData,
) {
    let period = (ENDTIME as i32) / (TIC_PCM_SIZE as i32);

    data.time = 0;
    while data.time < ENDTIME as i32 {
        let amp = get_amp(
            MAX_VOLUME as i32,
            (pcm.data[data.phase as usize] as i32) * SHRT_MAX / UCHAR_MAX,
        );
        update_amp(blip, data, amp);
        data.time += period;
        data.phase = (data.phase + 1) % (TIC_PCM_SIZE as i32);
    }
}

// ---------------------------------------------------------------------------
// SFX system
// ---------------------------------------------------------------------------

pub fn calc_loop_pos(loop_info: &SoundLoop, pos: i32) -> i32 {
    if loop_info.size > 0 {
        let mut offset = 0;
        for _ in 0..pos {
            if offset < (loop_info.start as i32 + loop_info.size as i32 - 1) {
                offset += 1;
            } else {
                offset = loop_info.start as i32;
            }
        }
        offset
    } else {
        if pos >= SFX_TICKS as i32 { (SFX_TICKS - 1) as i32 } else { pos }
    }
}

pub fn sfx_tick(
    sample: &Sample,
    channel: &mut ChannelData,
    note: i32,
    pitch: i32,
) -> (i32, u8, u8, u8, u8) {
    // returns (new_freq, volume, wave_idx, stereo_left, stereo_right)
    if channel.duration > 0 {
        channel.duration -= 1;
    }

    if channel.index < 0 || channel.duration == 0 {
        channel.tick = -1;
        channel.pos = [-1; 4];
        return (-1, 0, 0, 0, 0);
    }

    let pos = tools::sfx_pos(channel.speed as i32, channel.tick + 1);
    channel.tick = channel.tick.wrapping_add(1);

    // Update positions
    for i in 0..4 {
        channel.pos[i] = calc_loop_pos(&sample.loops[i], pos) as i16;
    }

    let pi = channel.pos[0] as usize; // volume index
    let volume = MAX_VOLUME as u8 - sample.data[pi].volume;

    if volume > 0 {
        let arp = (sample.data[channel.pos[1] as usize].chord as i32)
            * if sample.reverse != 0 { -1 } else { 1 };
        let note = if arp != 0 {
            (note + arp).clamp(0, NOTE_FREQS.len() as i32 - 1)
        } else {
            note
        };

        let freq = (NOTE_FREQS[note as usize] as i32)
            + (sample.data[channel.pos[2] as usize].pitch as i32)
                * if sample.pitch16x != 0 { 16 } else { 1 }
            + pitch;

        let wave_idx = sample.data[channel.pos[3] as usize].wave as usize;
        let stereo_left = if sample.stereo_left != 0 { 0 } else { channel.volume_left };
        let stereo_right = if sample.stereo_right != 0 { 0 } else { channel.volume_right };

        (freq, volume, wave_idx as u8, stereo_left, stereo_right)
    } else {
        (-1, 0, 0, 0, 0)
    }
}

// ---------------------------------------------------------------------------
// Music tracker
// ---------------------------------------------------------------------------

pub fn get_tempo(core_tempo: i32, track_tempo: i8) -> i32 {
    if core_tempo < 0 {
        track_tempo as i32 + DEFAULT_TEMPO
    } else {
        core_tempo
    }
}

pub fn get_speed(core_speed: i32, track_speed: i8) -> i32 {
    if core_speed < 0 {
        track_speed as i32 + DEFAULT_SPEED
    } else {
        core_speed
    }
}

pub fn tick2row(core_tempo: i32, core_speed: i32, track: &Track, tick: i32) -> i32 {
    let speed = get_speed(core_speed, track.speed);
    if speed > 0 {
        tick * get_tempo(core_tempo, track.tempo) * DEFAULT_SPEED / speed / (NOTES_PER_MINUTE as i32)
    } else {
        0
    }
}

pub fn row2tick(core_tempo: i32, core_speed: i32, track: &Track, row: i32) -> i32 {
    let tempo = get_tempo(core_tempo, track.tempo);
    if tempo > 0 {
        row * get_speed(core_speed, track.speed) * (NOTES_PER_MINUTE as i32) / tempo / DEFAULT_SPEED
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_freqs_size() {
        assert_eq!(NOTE_FREQS.len(), 104);
    }

    #[test]
    fn freq2period_zero() {
        assert_eq!(freq2period(0), 4096);
    }

    #[test]
    fn freq2period_basic() {
        let p = freq2period(440);
        assert!(p >= 10 && p <= 4096);
    }

    #[test]
    fn get_amp_mid() {
        // volume=8, amp=SHRT_MAX → SHRT_MAX * 8 / 15 / 5
        let expected = SHRT_MAX * 8 / 15 / 5;
        assert_eq!(get_amp(8, SHRT_MAX), expected);
    }

    #[test]
    fn calc_loop_no_loop() {
        let loop_info = SoundLoop { start: 0, size: 0 };
        assert_eq!(calc_loop_pos(&loop_info, 0), 0);
        assert_eq!(calc_loop_pos(&loop_info, 63), 63);
        assert_eq!(calc_loop_pos(&loop_info, 100), 63); // clamped to SFX_TICKS-1
    }

    #[test]
    fn calc_loop_with_loop() {
        let loop_info = SoundLoop { start: 2, size: 4 };
        // pos=0: loop doesn't execute, offset=0
        // pos=1: offset 0→1, pos=2: 1→2, pos=3: 2→3, pos=4: 3→4
        // pos=5: 4→5, pos=6: 5→2 (wrap)
        assert_eq!(calc_loop_pos(&loop_info, 0), 0);
        assert_eq!(calc_loop_pos(&loop_info, 1), 1);
        assert_eq!(calc_loop_pos(&loop_info, 2), 2);
        assert_eq!(calc_loop_pos(&loop_info, 3), 3);
        assert_eq!(calc_loop_pos(&loop_info, 4), 4);
        assert_eq!(calc_loop_pos(&loop_info, 5), 5);
        assert_eq!(calc_loop_pos(&loop_info, 6), 2);
    }

    #[test]
    fn param2val_basic() {
        let row = TrackRow([0x12, 0x34, 0x00]);
        // param1=0x1, param2=0x4 → 0x14
        assert_eq!(param2val(&row), 0x14);
    }

    #[test]
    fn get_tempo_default() {
        assert_eq!(get_tempo(-1, 5), 5 + DEFAULT_TEMPO);
    }

    #[test]
    fn get_tempo_explicit() {
        assert_eq!(get_tempo(130, 5), 130);
    }

    #[test]
    fn get_speed_default() {
        assert_eq!(get_speed(-1, 0), 0 + DEFAULT_SPEED);
    }

    #[test]
    fn track_row_sfx_id() {
        // sfxhi=bit7 of byte1, sfxlow=bits0-4 of byte2
        let row = TrackRow([0x00, 0x80, 0x15]);
        // sfxhi=1, sfxlow=0x15=21 → 1<<5|21 = 53
        assert_eq!(row.sfx(), 53);
    }

    #[test]
    fn track_row_components() {
        let row = TrackRow([0xAB, 0xCD, 0xEF]);
        // byte0: note=0x0B, param1=0xA
        // byte1: param2=0xD, command=(0xC>>4)&7=0xC&7? 0xC=1100, >>4=0? no
        // command = (0xCD >> 4) & 0x07 = 0x0C & 7 = 4
        // sfxhi = 0xCD >> 7 = 1
        // byte2: sfxlow=0x6F & 0x1F = 0xF=15, octave=(0xEF>>5)&7=7
        assert_eq!(row.note(), 0x0B);
        assert_eq!(row.param1(), 0x0A);
        assert_eq!(row.param2(), 0x0D);
        assert_eq!(row.command(), 4);
        assert_eq!(row.sfxhi(), 1);
        assert_eq!(row.sfxlow(), 0x0F);
        assert_eq!(row.octave(), 7);
    }
}
