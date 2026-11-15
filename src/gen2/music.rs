//! Music, played the way the games' own sound engine plays it.
//!
//! Once per frame, the engine reads each channel's music data and decides
//! what the Game Boy's four sound channels should be doing, which is ported
//! here from `audio/engine.asm` in [pret/pokecrystal], quirks and all. What
//! it decides is then played by a small model of those four channels.
//!
//! [pret/pokecrystal]: https://github.com/pret/pokecrystal

/// The Game Boy shows (and the engine runs) 59.73 frames per second.
const FRAME_RATE: f64 = 4_194_304.0 / 70_224.0;

/// The hardware steps volume envelopes 64 times per second.
const ENVELOPE_RATE: f64 = 64.0;

/// Samples are made at this many times the output rate and averaged, to
/// keep high notes from aliasing.
const OVERSAMPLING: u32 = 4;

/// After the last channel ends, notes still ringing get this long to fade.
const TAIL_SECONDS: f64 = 2.0;

/// Commands to read without reaching a note before giving up on a channel
/// (music data that loops without ever playing anything).
const MAX_COMMANDS_PER_NOTE: usize = 10_000;

const FIRST_MUSIC_CMD: u8 = 0xd0;
const SOUND_RET_CMD: u8 = 0xff;

const NOISE_CHANNEL: usize = 3;

/// Where the engine's tables are in a ROM.
#[derive(Debug, Clone, Copy)]
struct Tables {
    /// The engine's bank, which also holds its drum kits.
    bank: u8,
    frequencies: usize,
    wave_samples: usize,
    drumkits: usize,
}

impl Tables {
    /// The engine's tables are in the same order in every version of
    /// Gold, Silver and Crystal, starting with the note frequencies.
    fn find(rom: &[u8]) -> Option<Tables> {
        const FREQUENCY_TABLE_START: [u8; 8] = [0x00, 0x00, 0x2c, 0xf8, 0x9d, 0xf8, 0x07, 0xf9];
        const FREQUENCY_TABLE_LEN: usize = 25 * 2;
        const WAVE_SAMPLES_LEN: usize = 10 * 16;

        let frequencies = rom
            .windows(FREQUENCY_TABLE_START.len())
            .position(|window| window == FREQUENCY_TABLE_START)?;
        let wave_samples = frequencies + FREQUENCY_TABLE_LEN;

        Some(Tables {
            bank: (frequencies / 0x4000) as u8,
            frequencies,
            wave_samples,
            drumkits: wave_samples + WAVE_SAMPLES_LEN,
        })
    }
}

fn read(rom: &[u8], bank: u8, addr: u16) -> u8 {
    let pos = if addr < 0x4000 {
        addr as usize
    } else {
        (bank as usize) * 0x4000 + ((addr as usize) & 0x3fff)
    };
    rom.get(pos).copied().unwrap_or(0xff)
}

fn read_word(rom: &[u8], bank: u8, addr: u16) -> u16 {
    u16::from_le_bytes([read(rom, bank, addr), read(rom, bank, addr.wrapping_add(1))])
}

/// What happened to a channel this frame, telling the hardware what to update.
#[derive(Debug, Default, Clone, Copy)]
struct NoteFlags {
    rest: bool,
    /// A new note: restart the sound channel. (`NOTE_NOISE_SAMPLING`)
    trigger: bool,
    freq_override: bool,
    vibrato_override: bool,
    duty_override: bool,
}

/// One channel's state in the engine (`CHANNEL_*` in the disassembly).
#[derive(Debug, Default, Clone, Copy)]
struct Track {
    on: bool,
    bank: u8,
    addr: u16,
    last_addr: u16,
    in_subroutine: bool,
    looping: bool,
    loop_count: u8,
    condition: u8,
    sfx_notes: bool,
    noise: bool,

    note_duration: u8,
    duration_modifier: u8,
    note_length: u8,
    tempo: u16,
    octave: u8,
    transposition: u8,
    frequency: u16,
    duty: u8,
    duty_pattern: u8,
    duty_loop: bool,
    envelope: u8,
    pitch_offset: Option<u16>,

    vibrato: bool,
    vibrato_up: bool,
    vibrato_delay: u8,
    vibrato_delay_count: u8,
    vibrato_extent: u8,
    vibrato_rate: u8,

    pitch_slide: bool,
    pitch_slide_up: bool,
    pitch_slide_target: u16,
    pitch_slide_amount: u8,
    pitch_slide_fraction: u8,
    pitch_slide_accumulator: u8,

    flags: NoteFlags,
}

impl Track {
    fn new(bank: u8, addr: u16) -> Track {
        Track {
            on: true,
            bank,
            addr,
            tempo: 0x100,
            note_length: 1,
            ..Track::default()
        }
    }
}

/// What a channel plays this frame (`wCurTrack*`).
#[derive(Debug, Clone, Copy)]
struct Current {
    duty: u8,
    envelope: u8,
    frequency: u16,
}

/// The parts of the engine shared by all channels.
#[derive(Debug, Clone)]
struct Engine<'a> {
    rom: &'a [u8],
    tables: Tables,
    tracks: [Track; 4],
    /// `wCurNoteDuration`, the length of the next pitch slide.
    cur_note_duration: u8,
    drumkit: u8,
    noise_sample_addr: u16,
    noise_sample_delay: u8,
    volume: u8,
}

impl<'a> Engine<'a> {
    fn new(rom: &'a [u8], tables: Tables, bank: u8, addr: u16) -> Engine<'a> {
        let mut engine = Engine {
            rom,
            tables,
            tracks: [Track::default(); 4],
            cur_note_duration: 0,
            drumkit: 0,
            noise_sample_addr: 0,
            noise_sample_delay: 0,
            volume: 0x77,
        };

        let channel_count = (read(rom, bank, addr) >> 6) + 1;
        for i in 0..channel_count as u16 {
            engine.load_channel(bank, addr.wrapping_add(i * 3));
        }

        engine
    }

    /// Starts a channel from a channel header (`LoadChannel`).
    fn load_channel(&mut self, bank: u8, header: u16) {
        let id = (read(self.rom, bank, header) & 0x7) as usize;
        let addr = read_word(self.rom, bank, header.wrapping_add(1));

        // Channels 5-8 are for sound effects, which music has no business with.
        if let Some(track) = self.tracks.get_mut(id) {
            *track = Track::new(bank, addr);
        }
    }

    fn is_playing(&self) -> bool {
        self.tracks.iter().any(|t| t.on)
    }

    /// Runs one frame of the engine (`_UpdateSound`), updating the hardware.
    fn frame(&mut self, hardware: &mut Hardware) {
        for ch in 0..4 {
            if !self.tracks[ch].on {
                continue;
            }

            let track = &mut self.tracks[ch];
            if track.note_duration < 2 {
                track.vibrato_delay_count = track.vibrato_delay;
                track.pitch_slide = false;
                self.parse_music(ch);
            } else {
                track.note_duration -= 1;
            }

            self.apply_pitch_slide(ch);

            let track = &self.tracks[ch];
            let mut cur = Current {
                duty: track.duty,
                envelope: track.envelope,
                frequency: track.frequency,
            };

            self.handle_track_vibrato(ch, &mut cur);
            self.handle_noise(ch, &mut cur);
            let wave = self.wave_sample(cur.envelope & 0xf);
            hardware.update(ch, self.tracks[ch].flags, cur, wave);
            self.tracks[ch].flags = NoteFlags::default();
        }

        hardware.volume = self.volume;
    }

    /// One of the wave channel's waveforms (`WaveSamples`), as 32 4-bit samples.
    fn wave_sample(&self, index: u8) -> [u8; 32] {
        let mut wave = [0; 32];
        let start = self.tables.wave_samples + index as usize * 16;
        for (i, pair) in wave.chunks_mut(2).enumerate() {
            let byte = self.rom.get(start + i).copied().unwrap_or(0);
            pair[0] = byte >> 4;
            pair[1] = byte & 0xf;
        }
        wave
    }

    fn get_music_byte(&mut self, ch: usize) -> u8 {
        let track = &mut self.tracks[ch];
        let byte = read(self.rom, track.bank, track.addr);
        track.addr = track.addr.wrapping_add(1);
        byte
    }

    fn get_music_word(&mut self, ch: usize) -> u16 {
        u16::from_le_bytes([self.get_music_byte(ch), self.get_music_byte(ch)])
    }

    /// Reads music data until a note is read or the channel ends (`ParseMusic`).
    fn parse_music(&mut self, ch: usize) {
        for _ in 0..MAX_COMMANDS_PER_NOTE {
            let byte = self.get_music_byte(ch);

            if byte == SOUND_RET_CMD && !self.tracks[ch].in_subroutine {
                // The end of this channel.
                let track = &mut self.tracks[ch];
                track.on = false;
                track.flags.rest = true;
                return;
            }

            if byte >= FIRST_MUSIC_CMD {
                self.music_command(ch, byte);
                continue;
            }

            if self.tracks[ch].sfx_notes {
                self.parse_sfx_note(ch, byte);
            } else if self.tracks[ch].noise {
                self.get_noise_sample(ch, byte);
            } else {
                self.set_note_duration(ch, byte & 0xf);

                let pitch = byte >> 4;
                if pitch == 0 {
                    self.tracks[ch].flags.rest = true;
                } else {
                    let frequency = self.get_frequency(ch, self.tracks[ch].octave, pitch);
                    let track = &mut self.tracks[ch];
                    track.frequency = frequency;
                    track.flags.trigger = true;
                    self.load_note(ch);
                }
            }

            return;
        }

        log_stuck(ch);
        self.tracks[ch].on = false;
        self.tracks[ch].flags.rest = true;
    }

    /// A note written the way sound effects write them (`ParseSFXOrCry`).
    fn parse_sfx_note(&mut self, ch: usize, byte: u8) {
        self.tracks[ch].flags.trigger = true;
        self.set_note_duration(ch, byte);
        self.tracks[ch].envelope = self.get_music_byte(ch);
        let lo = self.get_music_byte(ch);
        let track = &mut self.tracks[ch];
        track.frequency = (track.frequency & 0xff00) | lo as u16;
        if ch != NOISE_CHANNEL {
            let hi = self.get_music_byte(ch);
            let track = &mut self.tracks[ch];
            track.frequency = (track.frequency & 0x00ff) | ((hi as u16) << 8);
        }
    }

    /// A drum note: starts a sample from the current drum kit (`GetNoiseSample`).
    fn get_noise_sample(&mut self, ch: usize, byte: u8) {
        if ch != NOISE_CHANNEL {
            return;
        }

        self.set_note_duration(ch, byte & 0xf);

        let bank = self.tables.bank;
        let drumkits = 0x4000 + (self.tables.drumkits % 0x4000) as u16;
        let drumkit = read_word(
            self.rom,
            bank,
            drumkits.wrapping_add(self.drumkit as u16 * 2),
        );

        let pitch = byte >> 4;
        if pitch == 0 {
            return;
        }

        self.noise_sample_addr = read_word(self.rom, bank, drumkit.wrapping_add(pitch as u16 * 2));
        self.noise_sample_delay = 0;
    }

    fn music_command(&mut self, ch: usize, cmd: u8) {
        match cmd {
            // octave
            0xd0..=0xd7 => self.tracks[ch].octave = cmd & 0x7,
            // note_type
            0xd8 => {
                self.tracks[ch].note_length = self.get_music_byte(ch);
                if ch != NOISE_CHANNEL {
                    self.tracks[ch].envelope = self.get_music_byte(ch);
                }
            }
            // transpose
            0xd9 => self.tracks[ch].transposition = self.get_music_byte(ch),
            // tempo
            0xda => {
                let tempo = u16::from_be_bytes([self.get_music_byte(ch), self.get_music_byte(ch)]);
                self.set_global_tempo(tempo);
            }
            // duty_cycle
            0xdb => self.tracks[ch].duty = self.get_music_byte(ch).rotate_right(2) & 0xc0,
            // volume_envelope
            0xdc => self.tracks[ch].envelope = self.get_music_byte(ch),
            // pitch_sweep (not modelled)
            0xdd => {
                self.get_music_byte(ch);
            }
            // duty_cycle_pattern
            0xde => {
                let pattern = self.get_music_byte(ch).rotate_right(2);
                let track = &mut self.tracks[ch];
                track.duty_loop = true;
                track.duty_pattern = pattern;
                track.duty = pattern & 0xc0;
            }
            // toggle_sfx
            0xdf => self.tracks[ch].sfx_notes = !self.tracks[ch].sfx_notes,
            // pitch_slide
            0xe0 => {
                self.cur_note_duration = self.get_music_byte(ch);
                let target = self.get_music_byte(ch);
                let target = self.get_frequency(ch, target >> 4, target & 0xf);
                let track = &mut self.tracks[ch];
                track.pitch_slide_target = target;
                track.pitch_slide = true;
            }
            // vibrato
            0xe1 => {
                let delay = self.get_music_byte(ch);
                let param = self.get_music_byte(ch);
                let extent = param >> 4;
                let rate = param & 0xf;
                let track = &mut self.tracks[ch];
                track.vibrato = true;
                track.vibrato_up = false;
                track.vibrato_delay = delay;
                track.vibrato_delay_count = delay;
                // Split in an upper and a lower half.
                track.vibrato_extent = (((extent >> 1) + (extent & 1)) << 4) | (extent >> 1);
                track.vibrato_rate = (rate << 4) | rate;
            }
            // toggle_noise, sfx_toggle_noise
            0xe3 | 0xf0 => {
                let track = &mut self.tracks[ch];
                if track.noise {
                    track.noise = false;
                } else {
                    track.noise = true;
                    let drumkit = self.get_music_byte(ch);
                    // The sound effects' drum kit is never used by music channels.
                    if cmd == 0xe3 {
                        self.drumkit = drumkit;
                    }
                }
            }
            // volume
            0xe5 => self.volume = self.get_music_byte(ch),
            // pitch_offset
            0xe6 => {
                let offset = u16::from_be_bytes([self.get_music_byte(ch), self.get_music_byte(ch)]);
                self.tracks[ch].pitch_offset = Some(offset);
            }
            // tempo_relative
            0xe9 => {
                let change = self.get_music_byte(ch) as i8;
                let tempo = self.tracks[ch].tempo.wrapping_add(change as i16 as u16);
                self.set_global_tempo(tempo);
            }
            // restart_channel
            0xea => {
                let header = self.get_music_word(ch);
                self.load_channel(self.tracks[ch].bank, header);
            }
            // new_song (unused by the games)
            0xeb => {
                self.get_music_word(ch);
            }
            // unknownmusic0xee: jumps if a condition the games never set is set
            0xee => {
                let track = &mut self.tracks[ch];
                track.addr = track.addr.wrapping_add(2);
            }
            // unknownmusic0xe2, force_stereo_panning, unknownmusic0xe7,
            // unknownmusic0xe8, stereo_panning: nothing to do in mono
            0xe2 | 0xe4 | 0xe7 | 0xe8 | 0xef => {
                self.get_music_byte(ch);
            }
            // sfx_priority_on/off, music0xf1-f9: nothing to do
            0xec | 0xed | 0xf1..=0xf9 => {}
            // set_condition
            0xfa => self.tracks[ch].condition = self.get_music_byte(ch),
            // sound_jump_if
            0xfb => {
                let condition = self.get_music_byte(ch);
                if condition == self.tracks[ch].condition {
                    self.tracks[ch].addr = self.get_music_word(ch);
                } else {
                    let track = &mut self.tracks[ch];
                    track.addr = track.addr.wrapping_add(2);
                }
            }
            // sound_jump
            0xfc => self.tracks[ch].addr = self.get_music_word(ch),
            // sound_loop
            0xfd => {
                let count = self.get_music_byte(ch);
                let track = &mut self.tracks[ch];

                if !track.looping {
                    if count == 0 {
                        // Forever.
                        self.tracks[ch].addr = self.get_music_word(ch);
                        return;
                    }
                    track.looping = true;
                    track.loop_count = count - 1;
                }

                if track.loop_count == 0 {
                    track.looping = false;
                    track.addr = track.addr.wrapping_add(2);
                } else {
                    track.loop_count -= 1;
                    self.tracks[ch].addr = self.get_music_word(ch);
                }
            }
            // sound_call
            0xfe => {
                let target = self.get_music_word(ch);
                let track = &mut self.tracks[ch];
                track.last_addr = track.addr;
                track.addr = target;
                track.in_subroutine = true;
            }
            // sound_ret, from a subroutine
            0xff => {
                let track = &mut self.tracks[ch];
                track.in_subroutine = false;
                track.addr = track.last_addr;
            }
            _ => unreachable!("not a music command: {cmd:02x}"),
        }
    }

    fn set_global_tempo(&mut self, tempo: u16) {
        for track in &mut self.tracks {
            track.tempo = tempo;
            track.duration_modifier = 0;
        }
    }

    /// `SetNoteDuration`, in frames, keeping the fraction for the next note.
    fn set_note_duration(&mut self, ch: usize, length: u8) {
        let track = &mut self.tracks[ch];
        let units = length.wrapping_add(1) as u16;
        let ticks = (track.note_length as u16).wrapping_mul(units) & 0xff;
        let total = track
            .tempo
            .wrapping_mul(ticks)
            .wrapping_add(track.duration_modifier as u16);
        track.duration_modifier = total as u8;
        track.note_duration = (total >> 8) as u8;
    }

    /// The hardware frequency of a note (`GetFrequency`).
    fn get_frequency(&self, ch: usize, octave: u8, pitch: u8) -> u16 {
        let transposition = self.tracks[ch].transposition;
        let mut octave = (transposition >> 4).wrapping_add(octave);
        let index = (transposition & 0xf) as usize + pitch as usize;

        let pos = self.tables.frequencies + index * 2;
        let mut frequency = u16::from_le_bytes([
            self.rom.get(pos).copied().unwrap_or(0),
            self.rom.get(pos + 1).copied().unwrap_or(0),
        ]);

        while octave < 7 {
            frequency = ((frequency as i16) >> 1) as u16;
            octave += 1;
        }

        frequency & 0x7ff
    }

    /// Works out how far each frame of a pitch slide goes (`LoadNote`).
    fn load_note(&mut self, ch: usize) {
        let track = &mut self.tracks[ch];
        if !track.pitch_slide {
            return;
        }

        self.cur_note_duration = track
            .note_duration
            .checked_sub(self.cur_note_duration)
            .unwrap_or(1);

        let distance = if track.frequency >= track.pitch_slide_target {
            track.pitch_slide_up = false;
            track.frequency - track.pitch_slide_target
        } else {
            track.pitch_slide_up = true;
            track.pitch_slide_target - track.frequency
        };

        // The engine divides by repeated subtraction, which never ends when
        // dividing by zero; this does.
        let duration = self.cur_note_duration.max(1) as u16;
        track.pitch_slide_amount = (distance / duration + 1) as u8;
        track.pitch_slide_fraction = (distance % duration) as u8;
        track.pitch_slide_accumulator = 0;
    }

    /// Slides the pitch a frame further (`ApplyPitchSlide`).
    fn apply_pitch_slide(&mut self, ch: usize) {
        let track = &mut self.tracks[ch];
        if !track.pitch_slide {
            return;
        }

        let (frequency, finished) = if track.pitch_slide_up {
            let mut frequency = track
                .frequency
                .wrapping_add(track.pitch_slide_amount as u16);
            let (accumulator, carry) = track
                .pitch_slide_accumulator
                .overflowing_add(track.pitch_slide_fraction);
            track.pitch_slide_accumulator = accumulator;
            frequency = frequency.wrapping_add(carry as u16);
            (frequency, track.pitch_slide_target < frequency)
        } else {
            let mut frequency = track
                .frequency
                .wrapping_sub(track.pitch_slide_amount as u16);
            // Doubling the fraction, rather than adding it up, is what the engine does.
            let (fraction, carry) = track
                .pitch_slide_fraction
                .overflowing_add(track.pitch_slide_fraction);
            track.pitch_slide_fraction = fraction;
            frequency = frequency.wrapping_sub(carry as u16);
            (frequency, frequency < track.pitch_slide_target)
        };

        if finished {
            track.pitch_slide = false;
            track.pitch_slide_up = false;
        } else {
            track.frequency = frequency;
            track.flags.freq_override = true;
            track.flags.duty_override = true;
        }
    }

    /// Duty cycle patterns, pitch offset and vibrato (`HandleTrackVibrato`).
    fn handle_track_vibrato(&mut self, ch: usize, cur: &mut Current) {
        let track = &mut self.tracks[ch];

        if track.duty_loop {
            track.duty_pattern = track.duty_pattern.rotate_left(2);
            cur.duty = track.duty_pattern & 0xc0;
            track.flags.duty_override = true;
        }

        if let Some(offset) = track.pitch_offset {
            cur.frequency = cur.frequency.wrapping_add(offset);
        }

        if !track.vibrato {
            return;
        }

        if track.vibrato_delay_count != 0 {
            track.vibrato_delay_count -= 1;
            return;
        }

        if track.vibrato_extent == 0 {
            return;
        }

        if track.vibrato_rate & 0xf != 0 {
            track.vibrato_rate -= 1;
            return;
        }

        track.vibrato_rate = (track.vibrato_rate & 0xf0) | (track.vibrato_rate >> 4);

        // Only the low byte of the frequency wobbles.
        let lo = cur.frequency as u8;
        let lo = if track.vibrato_up {
            track.vibrato_up = false;
            lo.saturating_sub(track.vibrato_extent & 0xf)
        } else {
            track.vibrato_up = true;
            lo.saturating_add(track.vibrato_extent >> 4)
        };

        cur.frequency = (cur.frequency & 0xff00) | lo as u16;
        track.flags.vibrato_override = true;
    }

    /// Steps through the current drum sample (`HandleNoise`).
    fn handle_noise(&mut self, ch: usize, cur: &mut Current) {
        if !self.tracks[ch].noise {
            return;
        }

        if self.noise_sample_delay > 0 {
            self.noise_sample_delay -= 1;
            return;
        }

        if self.noise_sample_addr == 0 {
            return;
        }

        let bank = self.tables.bank;
        let mut addr = self.noise_sample_addr;
        let length = read(self.rom, bank, addr);
        if length == SOUND_RET_CMD {
            return;
        }
        addr = addr.wrapping_add(1);

        self.noise_sample_delay = (length & 0xf) + 1;
        cur.envelope = read(self.rom, bank, addr);
        cur.frequency = read(self.rom, bank, addr.wrapping_add(1)) as u16;
        self.noise_sample_addr = addr.wrapping_add(2);
        self.tracks[ch].flags.trigger = true;
    }
}

#[cfg(debug_assertions)]
fn log_stuck(ch: usize) {
    eprintln!(
        "pokemon-synthesizer: music channel {} never reached a note",
        ch + 1
    );
}

#[cfg(not(debug_assertions))]
fn log_stuck(_ch: usize) {}

/// A volume envelope as the hardware runs it (`NRx2`).
#[derive(Debug, Default, Clone, Copy)]
struct Envelope {
    volume: u8,
    up: bool,
    period: u8,
    timer: u8,
}

impl Envelope {
    fn new(value: u8) -> Envelope {
        Envelope {
            volume: value >> 4,
            up: value & 0x8 != 0,
            period: value & 0x7,
            timer: value & 0x7,
        }
    }

    fn step(&mut self) {
        if self.period == 0 {
            return;
        }
        self.timer -= 1;
        if self.timer == 0 {
            self.timer = self.period;
            if self.up && self.volume < 15 {
                self.volume += 1;
            } else if !self.up && self.volume > 0 {
                self.volume -= 1;
            }
        }
    }
}

/// One of the four sound channels.
#[derive(Debug, Default, Clone, Copy)]
struct Voice {
    enabled: bool,
    duty: u8,
    frequency: u16,
    envelope: Envelope,
    /// How far through one period of the waveform, from 0 to 1.
    phase: f64,
    wave: [u8; 32],
    /// How far the wave channel's samples are shifted down; 4 is silent.
    wave_shift: u8,
    lfsr: u16,
    narrow: bool,
}

/// The sound hardware, as far as music needs it.
#[derive(Debug, Clone)]
struct Hardware {
    voices: [Voice; 4],
    /// `NR50`: left and right master volume.
    volume: u8,
}

impl Hardware {
    fn new() -> Hardware {
        Hardware {
            voices: [Voice::default(); 4],
            volume: 0x77,
        }
    }

    /// What the engine writes to the hardware for a channel (`UpdateChannels`).
    fn update(&mut self, ch: usize, flags: NoteFlags, cur: Current, wave: [u8; 32]) {
        let voice = &mut self.voices[ch];

        if flags.rest {
            voice.enabled = false;
            return;
        }

        if flags.trigger {
            match ch {
                0 | 1 => {
                    voice.duty = cur.duty >> 6;
                    voice.frequency = cur.frequency & 0x7ff;
                    voice.envelope = Envelope::new(cur.envelope);
                    // The channel is only on when its volume can be heard or rise.
                    voice.enabled = cur.envelope & 0xf8 != 0;
                }
                2 => {
                    voice.wave = wave;
                    // NR32's level, which the engine takes from the envelope's upper half.
                    voice.wave_shift = match ((cur.envelope << 1) >> 5) & 0x3 {
                        0 => 4,
                        level => level - 1,
                    };
                    voice.frequency = cur.frequency & 0x7ff;
                    voice.phase = 0.0;
                    voice.enabled = true;
                }
                _ => {
                    voice.envelope = Envelope::new(cur.envelope);
                    voice.frequency = cur.frequency & 0xff;
                    voice.narrow = cur.frequency & 0x8 != 0;
                    voice.lfsr = 0x7fff;
                    voice.enabled = cur.envelope & 0xf8 != 0;
                }
            }
            return;
        }

        match ch {
            0 => {
                if flags.freq_override {
                    voice.frequency = cur.frequency & 0x7ff;
                } else if flags.vibrato_override {
                    voice.duty = cur.duty >> 6;
                    voice.frequency = (voice.frequency & 0x700) | (cur.frequency & 0xff);
                    return;
                }
                if flags.duty_override {
                    voice.duty = cur.duty >> 6;
                }
            }
            1 => {
                if flags.vibrato_override {
                    voice.duty = cur.duty >> 6;
                    voice.frequency = (voice.frequency & 0x700) | (cur.frequency & 0xff);
                } else if flags.duty_override {
                    voice.duty = cur.duty >> 6;
                }
            }
            2 => {
                if flags.vibrato_override {
                    voice.frequency = (voice.frequency & 0x700) | (cur.frequency & 0xff);
                }
            }
            _ => {}
        }
    }

    fn step_envelopes(&mut self) {
        for voice in [0, 1, 3] {
            self.voices[voice].envelope.step();
        }
    }

    /// One sample of the mix, `dt` seconds after the last, from -1 to 1.
    fn sample(&mut self, dt: f64) -> f32 {
        let mut mix = 0.0;

        for (ch, voice) in self.voices.iter_mut().enumerate() {
            if !voice.enabled {
                continue;
            }

            mix += match ch {
                0 | 1 => {
                    let hz = 131_072.0 / (2048 - voice.frequency) as f64;
                    voice.phase = (voice.phase + hz * dt).fract();
                    let duty = [0.125, 0.25, 0.5, 0.75][voice.duty as usize & 0x3];
                    let level = voice.envelope.volume as f32 / 15.0;
                    if voice.phase < duty {
                        level
                    } else {
                        -level
                    }
                }
                2 => {
                    let hz = 65_536.0 / (2048 - voice.frequency) as f64;
                    voice.phase = (voice.phase + hz * dt).fract();
                    if voice.wave_shift >= 4 {
                        0.0
                    } else {
                        let sample = voice.wave[(voice.phase * 32.0) as usize & 31];
                        ((sample as f32 - 7.5) / 7.5) / (1 << voice.wave_shift) as f32
                    }
                }
                _ => {
                    let shift = voice.frequency >> 4;
                    if shift < 14 {
                        let divisor = match voice.frequency & 0x7 {
                            0 => 0.5,
                            divisor => divisor as f64,
                        };
                        let hz = 262_144.0 / divisor / (1u32 << shift) as f64;
                        voice.phase += hz * dt;
                        while voice.phase >= 1.0 {
                            voice.phase -= 1.0;
                            let bit = (voice.lfsr ^ (voice.lfsr >> 1)) & 1;
                            voice.lfsr = (voice.lfsr >> 1) | (bit << 14);
                            if voice.narrow {
                                voice.lfsr = (voice.lfsr & !(1 << 6)) | (bit << 6);
                            }
                        }
                    }
                    let level = voice.envelope.volume as f32 / 15.0;
                    if voice.lfsr & 1 == 0 {
                        level
                    } else {
                        -level
                    }
                }
            };
        }

        let left = ((self.volume >> 4) & 0x7) as f32 + 1.0;
        let right = (self.volume & 0x7) as f32 + 1.0;
        mix / 4.0 * (left + right) / 16.0
    }
}

/// A song, played from the start, at a sample rate of your choosing.
///
/// Most songs loop forever; those that don't end once they've faded out.
#[derive(Debug, Clone)]
pub struct MusicIterator<'a> {
    engine: Engine<'a>,
    hardware: Hardware,
    sample_rate: u32,
    until_frame: f64,
    until_envelope: f64,
    tail: f64,
}

impl<'a> MusicIterator<'a> {
    pub fn channels(&self) -> u16 {
        1
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

impl Iterator for MusicIterator<'_> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let rate = self.sample_rate as f64;

        if self.until_frame <= 0.0 {
            self.until_frame += rate / FRAME_RATE;
            self.engine.frame(&mut self.hardware);
        }
        self.until_frame -= 1.0;

        if self.until_envelope <= 0.0 {
            self.until_envelope += rate / ENVELOPE_RATE;
            self.hardware.step_envelopes();
        }
        self.until_envelope -= 1.0;

        if !self.engine.is_playing() {
            if self.tail <= 0.0 {
                return None;
            }
            self.tail -= 1.0 / rate;
        }

        let dt = 1.0 / (rate * OVERSAMPLING as f64);
        let mut sum = 0.0;
        for _ in 0..OVERSAMPLING {
            sum += self.hardware.sample(dt);
        }
        Some(sum / OVERSAMPLING as f32)
    }
}

/// Plays the song whose header is at `bank:addr`, such as one listed in
/// the `Music` table that the games' `PlayMusic` reads.
///
/// Returns `None` when `rom` doesn't have the Gold, Silver or Crystal
/// sound engine in it.
pub fn music(rom: &[u8], bank: u8, addr: u16, sample_rate: u32) -> Option<MusicIterator<'_>> {
    let tables = Tables::find(rom)?;

    Some(MusicIterator {
        engine: Engine::new(rom, tables, bank, addr),
        hardware: Hardware::new(),
        sample_rate,
        until_frame: 0.0,
        until_envelope: 0.0,
        tail: TAIL_SECONDS,
    })
}
