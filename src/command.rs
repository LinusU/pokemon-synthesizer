use crate::channel::ChannelType;

trait FromI4 {
    fn from_i4(data: u8) -> Self;
}

impl FromI4 for i8 {
    fn from_i4(data: u8) -> Self {
        let value = (data & 0b0111) as i8;

        if (data & 0b1000) == 0 {
            value
        } else {
            -value
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Note {
    CFlat,
    CSharp,
    DFlat,
    DSharp,
    EFlat,
    FFlat,
    FSharp,
    GFlat,
    GSharp,
    AFlat,
    ASharp,
    BFlat,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Command {
    PitchSweep {
        /// Length of time between pitch shifts. \
        /// Sometimes used with a value >7 in which case the MSB is ignored.
        length: u8,
        /// Positive value means increase in pitch, negative value means decrease in pitch. \
        /// Small magnitude means quick change, large magnitude means slow change. \
        /// In signed magnitude representation, so a value of 8 is the same as (negative) 0.
        change: i8,
    },
    SquareNote {
        length: u8,
        volume: u8,
        /// Positive value means decrease in volume, negative value means increase in volume.
        /// Small magnitude means quick change, large magnitude means slow change.
        /// In signed magnitude representation, so a value of 8 is the same as (negative) 0.
        fade: i8,
        freq: u16,
    },
    NoiseNote {
        length: u8,
        volume: u8,
        /// Positive value means decrease in volume, negative value means increase in volume.
        /// Small magnitude means quick change, large magnitude means slow change.
        /// In signed magnitude representation, so a value of 8 is the same as (negative) 0.
        fade: i8,
        value: u8,
    },
    Note {
        pitch: Note,
        length: u8,
    },
    DrumNote {
        instrument: u8,
        length: u8,
    },
    Rest(u8),
    NoteType {
        speed: u8,
        volume: u8,
        /// Positive value means decrease in volume, negative value means increase in volume.
        /// Small magnitude means quick change, large magnitude means slow change.
        /// In signed magnitude representation, so a value of 8 is the same as (negative) 0.
        fade: i8,
    },
    DrumSpeed(u8),
    Octave(u8),
    /// When enabled, effective frequency used is incremented by 1.
    TogglePerfectPitch,
    Vibrato {
        /// Time delay until vibrato effect begins.
        delay: u8,
        /// Amplitude of vibrato wave.
        depth: u8,
        /// Frequency of vibrato wave.
        rate: u8,
    },
    PitchSlide {
        length: u8,
        octave: u8,
        pitch: u8,
    },
    DutyCycle(u8),
    /// Used to calculate note delay counters, so a smaller value means music plays faster. \
    /// Ideally should be set to $100 or less to guarantee no overflow. \
    /// If larger than 0x100, large note speed or note length values might cause overflow. \
    /// Stored in big endian.
    Tempo(u16),
    Volume {
        left: u8,
        right: u8,
    },
    /// When enabled, the sfx data is interpreted as music data.
    ExecuteMusic,
    DutyCyclePattern(u8, u8, u8, u8),
    SoundCall(u16),
    Loop {
        count: u8,
        addr: u16,
    },
    Return,
}

impl Command {
    pub fn parse(rom: &[u8], bank: u8, addr: u16, channel: ChannelType) -> Command {
        let pos = ((bank as usize) * 0x4000) + ((addr as usize) & 0x3fff);

        match channel {
            ChannelType::MusicPulse => Command::parse_music_pulse(&rom[pos..]),
            ChannelType::MusicWave => Command::parse_music_wave(&rom[pos..]),
            ChannelType::MusicNoise => Command::parse_music_noise(&rom[pos..]),
            ChannelType::SfxPulse => Command::parse_sfx_pulse(&rom[pos..]),
            ChannelType::SfxWave => Command::parse_sfx_wave(&rom[pos..]),
            ChannelType::SfxNoise => Command::parse_sfx_noise(&rom[pos..]),
        }
    }

    #[rustfmt::skip]
    fn parse_music_pulse(data: &[u8]) -> Command {
        match data[0] {
            0x00..=0x0f => Command::Note { pitch: Note::CFlat, length: (data[0] & 0x0f) },
            0x10..=0x1f => Command::Note { pitch: Note::CSharp, length: (data[0] & 0x0f) },
            0x20..=0x2f => Command::Note { pitch: Note::DFlat, length: (data[0] & 0x0f) },
            0x30..=0x3f => Command::Note { pitch: Note::DSharp, length: (data[0] & 0x0f) },
            0x40..=0x4f => Command::Note { pitch: Note::EFlat, length: (data[0] & 0x0f) },
            0x50..=0x5f => Command::Note { pitch: Note::FFlat, length: (data[0] & 0x0f) },
            0x60..=0x6f => Command::Note { pitch: Note::FSharp, length: (data[0] & 0x0f) },
            0x70..=0x7f => Command::Note { pitch: Note::GFlat, length: (data[0] & 0x0f) },
            0x80..=0x8f => Command::Note { pitch: Note::GSharp, length: (data[0] & 0x0f) },
            0x90..=0x9f => Command::Note { pitch: Note::AFlat, length: (data[0] & 0x0f) },
            0xa0..=0xaf => Command::Note { pitch: Note::ASharp, length: (data[0] & 0x0f) },
            0xb0..=0xbf => Command::Note { pitch: Note::BFlat, length: (data[0] & 0x0f) },
            0xc0..=0xcf => Command::Rest(data[0] & 0x0f),
            0xd0..=0xdf => Command::NoteType { speed: (data[0] & 0x0f), volume: (data[1] & 0x0f), fade: i8::from_i4(data[1]) },
            0xe0..=0xe7 => Command::Octave(data[0] & 0x0f),
            0xe8 => Command::TogglePerfectPitch,
            0xea => Command::Vibrato { delay: data[1], depth: (data[2] >> 4), rate: (data[2] & 0x0f) },
            0xeb => Command::PitchSlide { length: data[1], octave: (data[2] >> 4), pitch: (data[2] & 0x0f) },
            0xec => Command::DutyCycle(data[1]),
            0xed => Command::Tempo(u16::from_be_bytes([data[1], data[2]])),
            0xf0 => Command::Volume { left: (data[1] >> 4), right: (data[1] & 0x0f) },
            0xf8 => Command::ExecuteMusic,
            0xfc => Command::DutyCyclePattern(data[1] >> 6, (data[1] >> 4) & 0x03, (data[1] >> 2) & 0x03, data[1] & 0x03),
            0xfd => Command::SoundCall(u16::from_le_bytes([data[1], data[2]])),
            0xfe => Command::Loop { count: data[1], addr: u16::from_le_bytes([data[2], data[3]]) },
            0xff => Command::Return,
            byte => todo!("Unknown music pulse command: {:02x}", byte)
        }
    }

    #[rustfmt::skip]
    fn parse_music_wave(data: &[u8]) -> Command {
        match data[0] {
            0x00..=0x0f => Command::Note { pitch: Note::CFlat, length: (data[0] & 0x0f) },
            0x10..=0x1f => Command::Note { pitch: Note::CSharp, length: (data[0] & 0x0f) },
            0x20..=0x2f => Command::Note { pitch: Note::DFlat, length: (data[0] & 0x0f) },
            0x30..=0x3f => Command::Note { pitch: Note::DSharp, length: (data[0] & 0x0f) },
            0x40..=0x4f => Command::Note { pitch: Note::EFlat, length: (data[0] & 0x0f) },
            0x50..=0x5f => Command::Note { pitch: Note::FFlat, length: (data[0] & 0x0f) },
            0x60..=0x6f => Command::Note { pitch: Note::FSharp, length: (data[0] & 0x0f) },
            0x70..=0x7f => Command::Note { pitch: Note::GFlat, length: (data[0] & 0x0f) },
            0x80..=0x8f => Command::Note { pitch: Note::GSharp, length: (data[0] & 0x0f) },
            0x90..=0x9f => Command::Note { pitch: Note::AFlat, length: (data[0] & 0x0f) },
            0xa0..=0xaf => Command::Note { pitch: Note::ASharp, length: (data[0] & 0x0f) },
            0xb0..=0xbf => Command::Note { pitch: Note::BFlat, length: (data[0] & 0x0f) },
            0xc0..=0xcf => Command::Rest(data[0] & 0x0f),
            0xd0..=0xdf => Command::NoteType { speed: (data[0] & 0x0f), volume: (data[1] & 0x0f), fade: i8::from_i4(data[1]) },
            0xe0..=0xe7 => Command::Octave(data[0] & 0x0f),
            0xe8 => Command::TogglePerfectPitch,
            0xea => Command::Vibrato { delay: data[1], depth: (data[2] >> 4), rate: (data[2] & 0x0f) },
            0xeb => Command::PitchSlide { length: data[1], octave: (data[2] >> 4), pitch: (data[2] & 0x0f) },
            0xed => Command::Tempo(u16::from_be_bytes([data[1], data[2]])),
            0xf8 => Command::ExecuteMusic,
            0xfd => Command::SoundCall(u16::from_le_bytes([data[1], data[2]])),
            0xfe => Command::Loop { count: data[1], addr: u16::from_le_bytes([data[2], data[3]]) },
            0xff => Command::Return,
            byte => todo!("Unknown music wave command: {:02x}", byte)
        }
    }

    #[rustfmt::skip]
    fn parse_music_noise(data: &[u8]) -> Command {
        match data[0] {
            0xb0..=0xbf => Command::DrumNote { instrument: data[1], length: (data[0] >> 4) },
            0xc0..=0xcf => Command::Rest(data[0] & 0x0f),
            0xd0..=0xdf => Command::DrumSpeed(data[0] & 0x0f),
            0xfd => Command::SoundCall(u16::from_le_bytes([data[1], data[2]])),
            0xfe => Command::Loop { count: data[1], addr: u16::from_le_bytes([data[2], data[3]]) },
            0xff => Command::Return,
            byte => todo!("Unknown music noise command: {:02x}", byte)
        }
    }

    #[rustfmt::skip]
    fn parse_sfx_pulse(data: &[u8]) -> Command {
        match data[0] {
            0x10 => Command::PitchSweep { length: (data[1] >> 4), change: i8::from_i4(data[1]) },
            0x20..=0x2f => Command::SquareNote { length: data[0] & 0x0f, volume: data[1] >> 4, fade: i8::from_i4(data[1]), freq: u16::from_le_bytes([data[2], data[3]]) },
            0xec => Command::DutyCycle(data[1]),
            0xf8 => Command::ExecuteMusic,
            0xfc => Command::DutyCyclePattern(data[1] >> 6, (data[1] >> 4) & 0x03, (data[1] >> 2) & 0x03, data[1] & 0x03),
            0xfd => Command::SoundCall(u16::from_le_bytes([data[1], data[2]])),
            0xfe => Command::Loop { count: data[1], addr: u16::from_le_bytes([data[2], data[3]]) },
            0xff => Command::Return,
            byte => todo!("Unknown SFX pulse channel command: {:02x}", byte)
        }
    }

    #[rustfmt::skip]
    fn parse_sfx_wave(data: &[u8]) -> Command {
        match data[0] {
            0xf8 => Command::ExecuteMusic,
            byte => todo!("Unknown SFX wave channel command: {:02x}", byte),
        }
    }

    #[rustfmt::skip]
    fn parse_sfx_noise(data: &[u8]) -> Command {
        match data[0] {
            0x20..=0x2f => Command::NoiseNote { length: data[0] & 0x0f, volume: data[1] >> 4, fade: i8::from_i4(data[1]), value: data[2] },
            0xec => Command::DutyCycle(data[1]),
            0xfc => Command::DutyCyclePattern(data[1] >> 6, (data[1] >> 4) & 0x03, (data[1] >> 2) & 0x03, data[1] & 0x03),
            0xfd => Command::SoundCall(u16::from_le_bytes([data[1], data[2]])),
            0xfe => Command::Loop { count: data[1], addr: u16::from_le_bytes([data[2], data[3]]) },
            0xff => Command::Return,
            byte => todo!("Unknown SFX noise channel command: {:02x}", byte),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Command::PitchSweep { .. } => 2,
            Command::SquareNote { .. } => 4,
            Command::NoiseNote { .. } => 3,
            Command::Note { .. } => 1,
            Command::DrumNote { .. } => 2,
            Command::Rest(_) => 1,
            Command::NoteType { .. } => 2,
            Command::DrumSpeed(_) => 1,
            Command::Octave(_) => 1,
            Command::TogglePerfectPitch => 1,
            Command::Vibrato { .. } => 3,
            Command::PitchSlide { .. } => 3,
            Command::DutyCycle(_) => 2,
            Command::Tempo(_) => 3,
            Command::Volume { .. } => 2,
            Command::ExecuteMusic => 1,
            Command::DutyCyclePattern(_, _, _, _) => 2,
            Command::SoundCall(_) => 3,
            Command::Loop { .. } => 4,
            Command::Return => 1,
        }
    }
}
