use crate::command::Command;

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
    fn to_muisc(&self) -> ChannelType {
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

#[derive(Debug)]
pub struct Channel<'a> {
    rom: &'a [u8],
    bank: u8,
    addr: u16,
    channel: ChannelType,
    id: u8,
}

impl Channel<'_> {
    pub fn new(rom: &[u8], bank: u8, addr: u16, channel: ChannelType, id: u8) -> Channel {
        Channel { rom, bank, addr, channel, id }
    }

    /// Returns the length of the channel in samples, without the fadeout of the last note.
    ///
    /// If the channel loops forever, returns None.
    pub fn len(&self, length: i8) -> Option<usize> {
        let mut result = 0;
        let mut leftovers = 0;

        let mut addr = self.addr;
        let mut channel = self.channel;
        let mut loop_counter = 1u8;

        loop {
            let cmd = Command::parse(self.rom, self.bank, addr, channel);

            match cmd {
                Command::Return => {
                    return Some(result);
                }

                Command::ExecuteMusic => {
                    channel = channel.to_muisc();
                }

                Command::DutyCycle(_) => {}
                Command::DutyCyclePattern(_, _, _, _) => {}

                Command::Loop { count, addr: target } => {
                    if count == 0 {
                        return None;
                    }

                    if loop_counter < count {
                        loop_counter += 1;
                        addr = target;
                        continue;
                    }
                }

                Command::SquareNote { length: cmd_len, .. } => {
                    let subframes =
                        (((length as isize) + 0x100) as usize * ((cmd_len as usize) + 1)) + leftovers;
                    let thisnote = SAMPLES_PER_FRAME * (subframes >> 8);

                    leftovers = subframes & 0xff;
                    result += thisnote;
                }

                Command::NoiseNote { length: cmd_len, .. } => {
                    let subframes =
                        (((length as isize) + 0x100) as usize * ((cmd_len as usize) + 1)) + leftovers;
                    let thisnote = SAMPLES_PER_FRAME * (subframes >> 8);

                    leftovers = subframes & 0xff;
                    result += thisnote;
                }

                _ => todo!("Sound length of {:?}", cmd),
            }

            addr += cmd.len() as u16;
        }
    }

    pub fn pcm(&self, pitch: u8, length: i8) -> ChannelIterator {
        ChannelIterator::new(self, pitch, length)
    }
}

pub struct ChannelIterator<'a> {
    rom: &'a [u8],
    bank: u8,
    addr: u16,
    channel: ChannelType,
    channel_id: u8,

    pitch: u8,
    length: i8,

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
}

impl<'a> ChannelIterator<'a> {
    fn new(channel: &'a Channel, pitch: u8, length : i8) -> ChannelIterator<'a> {
        Self {
            rom: channel.rom,
            bank: channel.bank,
            addr: channel.addr,
            channel: channel.channel,
            channel_id: channel.id,

            pitch,
            length,

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
        }
    }

    pub fn only_fadeout_left(&self) -> bool {
        self.is_done
    }

    pub fn reset_pitch(&mut self) {
        self.pitch = 0;
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
                            * (2048 - ((self.freq as usize + (self.pitch as usize)) & 0x7ff))
                            / 131072;

                        // apply this note
                        for index in 0..SAMPLES_PER_FRAME {
                            let enabled = calc_duty(self.duty & 0b11, self.period_count);
                            result[index] = sample(enabled as isize, self.volume as isize);

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

                        for index in 0..SAMPLES_PER_FRAME {
                            let bit0 = self.noise_buffer & 1;
                            result[index] = sample((1 ^ bit0) as isize, self.volume as isize);

                            // according to params, update buffer
                            if index
                                % ((2.0
                                    * (if divider == 0 { 0.5 } else { divider as f64 })
                                    * (1 << (shift + 1)) as f64)
                                    as usize)
                                == 0
                            {
                                let bit1 = (self.noise_buffer >> 1) & 1;
                                self.noise_buffer = (self.noise_buffer >> 1) | ((bit0 ^ bit1) << 14);
                                if width {
                                    self.noise_buffer = (self.noise_buffer >> 1) | ((bit0 ^ bit1) << 6);
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

                return Some(result);
            }

            // Read and process next command

            let cmd = Command::parse(self.rom, self.bank, self.addr, self.channel);
            eprintln!("Ch{} {:?}", self.channel_id, cmd);

            match cmd {
                Command::Return => {
                    self.is_done = true;
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

                Command::Loop { count, addr } => {
                    if count == 0 {
                        self.addr = addr;
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
                    let subframes = (((self.length as isize) + 0x100) as usize)
                        * (length as usize + 1)
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
                    let subframes = (((self.length as isize) + 0x100) as usize)
                        * (length as usize + 1)
                        + (self.note_delay_fraction as usize);

                    self.note_delay = (subframes >> 8) as u8;
                    self.note_delay_fraction = (subframes & 0xff) as u8;

                    self.volume = volume;
                    self.volume_fade = fade;
                    self.volume_fade_delay = (fade & 0b111) as u8;
                    self.noise_params = value.wrapping_add(self.pitch);
                    self.noise_buffer = 0x7fff;
                }

                _ => todo!("PCM data of {:?}", cmd)
            }

            self.addr += cmd.len() as u16;
        }
    }
}
