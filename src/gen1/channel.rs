use super::command::Command;

pub const SAMPLES_PER_FRAME: usize = 17556;
pub const SOURCE_SAMPLE_RATE: usize = 1048576;

fn calc_duty(duty: u8, period_count: f64) -> bool {
    match duty {
        0 => (0.5..0.625).contains(&period_count),
        1 => (0.5..0.75).contains(&period_count),
        2 => (0.5..0.875).contains(&period_count),
        3 => !(0.5..0.875).contains(&period_count),
        _ => panic!("Invalid duty cycle: {}", duty),
    }
}

fn sample(bin: isize, volume: isize) -> f32 {
    (((2 * bin) - 1) as f32) * (((volume as f32) * -1.0) / 16.0)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ChannelType {
    MusicPulse,
    MusicWave,
    MusicNoise,
    SfxPulse,
    SfxWave,
    SfxNoise,
}

impl ChannelType {
    fn to_muisc(self) -> ChannelType {
        match self {
            ChannelType::MusicPulse => ChannelType::MusicPulse,
            ChannelType::MusicWave => ChannelType::MusicWave,
            ChannelType::MusicNoise => ChannelType::MusicNoise,
            ChannelType::SfxPulse => ChannelType::MusicPulse,
            ChannelType::SfxWave => ChannelType::MusicWave,
            ChannelType::SfxNoise => ChannelType::MusicNoise,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Channel<'a> {
    rom: &'a [u8],
    bank: u8,
    addr: u16,
    channel: ChannelType,
}

impl<'a> Channel<'a> {
    pub fn new(rom: &[u8], bank: u8, addr: u16, channel: ChannelType) -> Channel {
        Channel {
            rom,
            bank,
            addr,
            channel,
        }
    }

    pub fn pcm(self, pitch: i8, length: u16) -> ChannelIterator<'a> {
        ChannelIterator::new(self, pitch, length)
    }
}

#[derive(Debug, Clone)]
pub struct ChannelIterator<'a> {
    rom: &'a [u8],
    bank: u8,
    addr: u16,
    channel: ChannelType,

    length: u16,

    pitch: i8,
    pitch_sweep: i8,
    pitch_sweep_delay: u8,
    pitch_sweep_period: u8,

    loop_counter: u8,
    note_delay: u8,
    note_delay_fraction: u8,

    duty: u8,
    volume: u8,
    volume_fade: i8,
    volume_fade_delay: u8,
    freq: u16,

    noise_params: u8,
    noise_buffer: u16,

    period_count: f64,
    is_done: bool,

    is_infinite: Option<bool>,
}

impl<'a> ChannelIterator<'a> {
    fn new(channel: Channel<'a>, pitch: i8, length: u16) -> ChannelIterator<'a> {
        Self {
            rom: channel.rom,
            bank: channel.bank,
            addr: channel.addr,
            channel: channel.channel,

            length,

            pitch,
            pitch_sweep: 0,
            pitch_sweep_delay: 0,
            pitch_sweep_period: 0,

            loop_counter: 1,
            note_delay: 0,
            note_delay_fraction: 0,

            duty: 0,
            volume: 0,
            volume_fade: 0,
            volume_fade_delay: 0,
            freq: 0,

            noise_params: 0,
            noise_buffer: 0x7fff,

            period_count: 0.0,
            is_done: false,

            is_infinite: None,
        }
    }

    pub fn only_fadeout_left(&self) -> bool {
        self.is_done
    }

    pub fn reset_pitch(&mut self) {
        self.pitch = 0;
    }

    pub fn is_infinite(&self) -> Option<bool> {
        self.is_infinite
    }
}

impl Iterator for ChannelIterator<'_> {
    type Item = [f32; SAMPLES_PER_FRAME];

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Generate sound data
            if self.note_delay > 0 || self.is_done {
                if self.is_done && self.volume == 0 {
                    return None;
                }

                let mut result = [0.0; SAMPLES_PER_FRAME];

                match self.channel {
                    ChannelType::SfxPulse => {
                        // number of samples for a single period of the note's pitch
                        let period = SOURCE_SAMPLE_RATE
                            * (2048
                                - ((self.freq as usize + ((self.pitch as u8) as usize)) & 0x7ff))
                            / 131072;

                        // apply this note
                        for data in result.iter_mut() {
                            let enabled = calc_duty(self.duty & 0b11, self.period_count);
                            *data = sample(enabled as isize, self.volume as isize);

                            self.period_count += 1.0 / (period as f64);

                            if self.period_count >= 1.0 {
                                self.period_count -= 1.0;
                            }
                        }

                        // once per frame, adjust duty
                        self.duty = self.duty.rotate_left(2);
                    }

                    ChannelType::SfxNoise => {
                        let shift = self.noise_params >> 4;
                        let shift = if shift > 0xd { shift & 0xd } else { shift }; // not sure how to deal with E or F, but its so low you can hardly notice it anyway

                        let divider = self.noise_params & 0x7;
                        let width = (self.noise_params & 0x8) == 0x8;

                        for (index, data) in result.iter_mut().enumerate() {
                            let bit0 = self.noise_buffer & 1;
                            *data = sample((1 ^ bit0) as isize, self.volume as isize);

                            // according to params, update buffer
                            if index
                                % ((2.0
                                    * (if divider == 0 { 0.5 } else { divider as f64 })
                                    * (1 << (shift + 1)) as f64)
                                    as usize)
                                == 0
                            {
                                let bit1 = (self.noise_buffer >> 1) & 1;
                                self.noise_buffer =
                                    (self.noise_buffer >> 1) | ((bit0 ^ bit1) << 14);
                                if width {
                                    self.noise_buffer =
                                        (self.noise_buffer >> 1) | ((bit0 ^ bit1) << 6);
                                }
                            }
                        }
                    }

                    channel => todo!("Channel {:?}", channel),
                }

                if self.note_delay > 0 {
                    self.note_delay -= 1;
                }

                // once per frame * fadeamount, adjust volume
                match self.volume_fade_delay {
                    0 => {}
                    1 => {
                        self.volume_fade_delay = (self.volume_fade & 0b111) as u8;

                        if self.volume_fade < 0 && self.volume < 15 {
                            self.volume += 1;
                        } else if self.volume_fade > 0 && self.volume > 0 {
                            self.volume -= 1;
                        }
                    }
                    _ => {
                        self.volume_fade_delay -= 1;
                    }
                }

                // once per frame * fadeamount, adjust pitch
                match self.pitch_sweep_delay {
                    0 => {}
                    1 => {
                        self.pitch_sweep_delay = self.pitch_sweep_period;
                        let offset = self.freq >> self.pitch_sweep.unsigned_abs();

                        if self.pitch_sweep < 0 {
                            self.freq = self.freq.wrapping_sub(offset);
                        } else {
                            self.freq = self.freq.wrapping_add(offset);
                        }
                    }
                    _ => {
                        self.pitch_sweep_delay -= 1;
                    }
                }

                return Some(result);
            }

            // Read and process next command

            let cmd = Command::parse(self.rom, self.bank, self.addr, self.channel);

            match cmd {
                Command::Return => {
                    self.is_done = true;
                    self.is_infinite = Some(false);
                    continue;
                }

                Command::ExecuteMusic => {
                    self.channel = self.channel.to_muisc();
                }

                Command::DutyCycle(a) => {
                    self.duty = (a << 6) | (a << 4) | (a << 2) | a;
                }

                Command::DutyCyclePattern(a, b, c, d) => {
                    self.duty = (a << 6) | (b << 4) | (c << 2) | d;
                }

                Command::PitchSweep { length, change } => {
                    self.pitch_sweep = change;
                    self.pitch_sweep_delay = length;
                    self.pitch_sweep_period = length;
                }

                Command::Loop { count, addr } => {
                    if count == 0 {
                        self.addr = addr;
                        self.is_infinite = Some(true);
                        continue;
                    }

                    if self.loop_counter < count {
                        self.loop_counter += 1;
                        self.addr = addr;
                        continue;
                    }
                }

                Command::SquareNote {
                    length,
                    volume,
                    fade,
                    freq,
                } => {
                    // number of samples for this single note
                    let subframes = (self.length as usize) * (length as usize + 1)
                        + (self.note_delay_fraction as usize);

                    self.note_delay = (subframes >> 8) as u8;
                    self.note_delay_fraction = (subframes & 0xff) as u8;

                    self.volume = volume;
                    self.volume_fade = fade;
                    self.volume_fade_delay = (fade & 0b111) as u8;
                    self.freq = freq;
                }

                Command::NoiseNote {
                    length,
                    volume,
                    fade,
                    value,
                } => {
                    // number of samples for this single note
                    let subframes = (self.length as usize) * (length as usize + 1)
                        + (self.note_delay_fraction as usize);

                    self.note_delay = (subframes >> 8) as u8;
                    self.note_delay_fraction = (subframes & 0xff) as u8;

                    self.volume = volume;
                    self.volume_fade = fade;
                    self.volume_fade_delay = (fade & 0b111) as u8;
                    self.noise_params = value.wrapping_add(self.pitch as u8);
                    self.noise_buffer = 0x7fff;
                }

                _ => todo!("PCM data of {:?}", cmd),
            }

            self.addr += cmd.len() as u16;
        }
    }
}
