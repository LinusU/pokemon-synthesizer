use std::collections::VecDeque;

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
    duty: u8,
    period_count: f64,
    leftovers: usize,
    loop_counter: u8,
    note_counter: u8,
    buffer: VecDeque<f32>,
    // is_done_in: Option<usize>,
    is_done: bool,
}

impl<'a> ChannelIterator<'a> {
    fn new(channel: &'a Channel, pitch: u8, length : i8) -> ChannelIterator<'a> {
        Self {
            rom: channel.rom,
            bank: channel.bank,
            addr: channel.addr,
            channel: channel.channel,
            pitch,
            length,
            duty: 0,
            period_count: 0.0,
            leftovers: 0,
            loop_counter: 1,
            note_counter: 0,
            buffer: VecDeque::new(),
            // is_done_in: None,
            is_done: false,
            channel_id: channel.id,
        }
    }

    pub fn only_fadeout_left(&self) -> bool {
        self.is_done
    }

    pub fn reset_pitch(&mut self) {
        eprintln!("Resetting pitch, the buffer length is {}", self.buffer.len());
        self.pitch = 0;
    }
}

impl Iterator for ChannelIterator<'_> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(sample) = self.buffer.pop_front() {
                return Some(sample);
            }

            if self.is_done {
                return None;
            }

            let cmd = Command::parse(self.rom, self.bank, self.addr, self.channel);

            // FIXME: This will read some extra bytes at the end of the song
            let mut is_last_command = Command::parse(self.rom, self.bank, self.addr + (cmd.len() as u16), self.channel) == Command::Return;

            if !is_last_command {
                if let Command::Loop { count, .. } = Command::parse(self.rom, self.bank, self.addr + (cmd.len() as u16), self.channel)    {
                    if count != 0 && self.loop_counter == count {
                        is_last_command = Command::parse(self.rom, self.bank, self.addr + (cmd.len() as u16) + 4, self.channel) == Command::Return;
                    }
                }
            }

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
                    length: n_samples_per_note,
                    volume,
                    fade,
                    freq,
                } => {
                    let mut volume = volume as isize;

                    eprintln!("Ch{} Note {:?} at {:02x}:{:04x}", self.channel_id, cmd, self.bank, self.addr);

                    // number of samples for this single note
                    let subframes = (((self.length as isize) + 0x100) as usize)
                        // * (n_samples_per_note as usize + 1)
                        + self.leftovers;

                    let sample_count = SAMPLES_PER_FRAME * (subframes >> 8);

                    self.leftovers = subframes & 0xff;

                    // number of samples for a single period of the note's pitch
                    let period = SOURCE_SAMPLE_RATE
                        * (2048 - ((freq as usize + (self.pitch as usize)) & 0x7ff))
                        / 131072;

                    // if is_last_command && self.note_counter == (n_samples_per_note - 1) {
                    //     eprintln!("Ch{} Entering the last but one", self.channel_id);
                    //     self.is_done_in = Some(0);
                    // }

                    // apply this note
                    for index in 0..2500000 {
                        // if sample_count > 0 && index == sample_count && is_last_command && self.note_counter == n_samples_per_note {
                        //     eprintln!("Note {:?} is done in {} samples", cmd, self.buffer.len());
                        //     // self.is_done_in = Some(self.buffer.len());
                        //     self.is_done_in = Some(0);
                        // }

                        if index >= sample_count && !(is_last_command && self.note_counter == n_samples_per_note && volume > 0) {
                            break;
                        }

                        let enabled = calc_duty(self.duty & 0b11, self.period_count);
                        self.buffer.push_back(sample(enabled as isize, volume));

                        self.period_count += 1.0 / (period as f64);

                        if self.period_count >= 1.0 {
                            self.period_count -= 1.0;
                        }

                        // once per frame, adjust duty
                        if index < sample_count && self.buffer.len() % SAMPLES_PER_FRAME == 0 {
                            self.duty = self.duty.rotate_left(2);
                        }

                        // once per frame * fadeamount, adjust volume
                        if fade != 0
                            && ((index + 1) % (SAMPLES_PER_FRAME * (fade.unsigned_abs() as usize)))
                                == 0
                        {
                            volume += if fade < 0 { 1 } else { -1 };
                            volume = volume.clamp(0, 0x0f);
                        }
                    }

                    if self.note_counter < n_samples_per_note {
                        self.note_counter += 1;
                        continue;
                    } else {
                        self.note_counter = 0;
                    }
                }

                Command::NoiseNote {
                    length: n_samples_per_note,
                    volume,
                    fade,
                    value,
                } => {
                    // number of samples for this single note
                    let subframes = (((self.length as isize) + 0x100) as usize)
                        // * (n_samples_per_note as usize + 1)
                        + self.leftovers;
                    let sample_count = SAMPLES_PER_FRAME * (subframes >> 8);
                    self.leftovers = subframes & 0xff;

                    // volume and fade control
                    let mut volume = volume as isize;
                    let params = value.wrapping_add(self.pitch);

                    // apply this note
                    let shift = params >> 4;
                    let shift = if shift > 0xd { shift & 0xd } else { shift }; // not sure how to deal with E or F, but its so low you can hardly notice it anyway

                    let divider = params & 0x7;
                    let width = (params & 0x8) == 0x8;
                    let mut noise_buffer: u16 = 0x7fff;

                    for index in 0..2500000 {
                        // if index == sample_count && !(is_last_command && self.note_counter == n_samples_per_note && volume > 0) {
                        //     eprintln!("Note {:?} is done in {} samples", cmd, self.buffer.len());
                        // }
                        if index >= sample_count && !(is_last_command && self.note_counter == n_samples_per_note && volume > 0) {
                            break;
                        }

                        let bit0 = noise_buffer & 1;
                        self.buffer.push_back(sample((1 ^ bit0) as isize, volume));

                        // according to params, update buffer
                        if self.buffer.len()
                            % ((2.0
                                * (if divider == 0 { 0.5 } else { divider as f64 })
                                * (1 << (shift + 1)) as f64)
                                as usize)
                            == 0
                        {
                            let bit1 = (noise_buffer >> 1) & 1;
                            noise_buffer = (noise_buffer >> 1) | ((bit0 ^ bit1) << 14);
                            if width {
                                noise_buffer = (noise_buffer >> 1) | ((bit0 ^ bit1) << 6);
                            }
                        }

                        // once per frame * fadeamount, adjust volume
                        if fade != 0
                            && ((index + 1) % (SAMPLES_PER_FRAME * (fade.unsigned_abs() as usize)))
                                == 0
                        {
                            volume += if fade < 0 { 1 } else { -1 };
                            volume = volume.clamp(0, 0x0f);
                        }
                    }

                    if self.note_counter < n_samples_per_note {
                        self.note_counter += 1;
                        continue;
                    } else {
                        self.note_counter = 0;
                    }
                }

                _ => todo!("PCM data of {:?}", cmd)
            }

            self.addr += cmd.len() as u16;
        }
    }
}
