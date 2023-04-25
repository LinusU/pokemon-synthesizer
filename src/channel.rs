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
    Pulse,
    Noise,
}

#[derive(Debug)]
pub struct Channel {
    commands: Vec<Command>,
}

impl Channel {
    pub fn parse(rom: &[u8], bank: u8, addr: u16, channel: ChannelType) -> Channel {
        let mut pos = ((bank as usize) * 0x4000) + ((addr as usize) & 0x3fff);
        let mut commands = Vec::new();

        loop {
            let byte = rom[pos];

            pos += 1;

            match byte {
                0x20..=0x2f => {
                    let length = byte & 0x0f;
                    let volume_and_fade = rom[pos];
                    pos += 1;

                    let volume = volume_and_fade >> 4;
                    let fade = volume_and_fade & 0x0f;

                    let fade = if fade & 0x08 != 0 {
                        -(fade as i8 & 0x07)
                    } else {
                        fade as i8
                    };

                    match channel {
                        ChannelType::Pulse => {
                            let freq = u16::from_le_bytes([rom[pos], rom[pos + 1]]);
                            pos += 2;

                            commands.push(Command::SquareNote {
                                length,
                                volume,
                                fade,
                                freq,
                            });
                        }

                        ChannelType::Noise => {
                            let value = rom[pos];
                            pos += 1;

                            commands.push(Command::NoiseNote {
                                length,
                                volume,
                                fade,
                                value,
                            });
                        }
                    }
                }

                0xec => {
                    commands.push(Command::DutyCycle(rom[pos]));
                    pos += 1;
                }

                0xfc => {
                    let byte = rom[pos];
                    commands.push(Command::DutyCyclePattern(
                        byte >> 6,
                        (byte >> 4) & 0x03,
                        (byte >> 2) & 0x03,
                        byte & 0x03,
                    ));
                    pos += 1;
                }

                0xfe => {
                    let repeat = rom[pos];
                    let ptr = u16::from_le_bytes([rom[pos + 1], rom[pos + 2]]);
                    pos += 3;

                    if repeat == 0 {
                        todo!("Infinite loops")
                    }

                    if ptr != addr {
                        todo!("Loop to other position than start");
                    }

                    let range = 0..commands.len();

                    for _ in 1..repeat {
                        commands.extend_from_within(range.clone());
                    }
                }

                0xff => break,
                _ => panic!("Invalid SFX command: {:02x}", byte),
            }
        }

        Channel { commands }
    }

    pub fn len(&self, length: i8) -> usize {
        let mut result = 0;
        let mut leftovers = 0;

        for cmd in &self.commands {
            if let Command::SquareNote {
                length: cmd_len, ..
            } = cmd
            {
                let subframes =
                    (((length as isize) + 0x100) as usize * ((*cmd_len as usize) + 1)) + leftovers;
                let thisnote = SAMPLES_PER_FRAME * (subframes >> 8);

                leftovers = subframes & 0xff;
                result += thisnote;
            }

            if let Command::NoiseNote {
                length: cmd_len, ..
            } = cmd
            {
                let subframes =
                    (((length as isize) + 0x100) as usize * ((*cmd_len as usize) + 1)) + leftovers;
                let thisnote = SAMPLES_PER_FRAME * (subframes >> 8);

                leftovers = subframes & 0xff;
                result += thisnote;
            }
        }

        result
    }

    pub fn pcm(&self, pitch: u8, length: i8, cutoff: Option<usize>) -> Vec<f32> {
        let mut result = Vec::new();

        if self.commands.is_empty() {
            return result;
        }

        let mut duty = 0;
        let mut leftovers = 0;
        let mut period_count = 0.0;

        let last_index = self.commands.len() - 1;

        for (index, command) in self.commands.iter().enumerate() {
            let is_last_command = index == last_index;

            match command {
                Command::DutyCycle(a) => {
                    duty = (a << 6) | (a << 4) | (a << 2) | a;
                }

                Command::DutyCyclePattern(a, b, c, d) => {
                    duty = (a << 6) | (b << 4) | (c << 2) | d;
                }

                Command::SquareNote {
                    length: n_samples_per_note,
                    volume,
                    fade,
                    freq,
                } => {
                    let mut volume = *volume as isize;

                    // number of samples for this single note
                    let subframes = (((length as isize) + 0x100) as usize)
                        * (*n_samples_per_note as usize + 1)
                        + leftovers;

                    let sample_count = SAMPLES_PER_FRAME * (subframes >> 8);

                    leftovers = subframes & 0xff;

                    // number of samples for a single period of the note's pitch
                    let period = SOURCE_SAMPLE_RATE
                        * (2048 - ((*freq as usize + (pitch as usize)) & 0x7ff))
                        / 131072;

                    // apply this note
                    for index in 0..2500000 {
                        if index >= sample_count && !(is_last_command && volume > 0) {
                            break;
                        }

                        let enabled = calc_duty(duty & 0b11, period_count);
                        result.push(sample(enabled as isize, volume));

                        period_count += 1.0 / (period as f64);

                        if period_count >= 1.0 {
                            period_count -= 1.0;
                        }

                        // once per frame, adjust duty
                        if index < sample_count && result.len() % SAMPLES_PER_FRAME == 0 {
                            duty = duty.rotate_left(2);
                        }

                        // once per frame * fadeamount, adjust volume
                        if *fade != 0
                            && ((index + 1) % (SAMPLES_PER_FRAME * (fade.unsigned_abs() as usize)))
                                == 0
                        {
                            volume += if *fade < 0 { 1 } else { -1 };
                            volume = volume.clamp(0, 0x0f);
                        }
                    }
                }

                Command::NoiseNote {
                    length: n_samples_per_note,
                    volume,
                    fade,
                    value,
                } => {
                    // number of samples for this single note
                    let subframes = (((length as isize) + 0x100) as usize)
                        * (*n_samples_per_note as usize + 1)
                        + leftovers;
                    let sample_count = SAMPLES_PER_FRAME * (subframes >> 8);
                    leftovers = subframes & 0xff;

                    // volume and fade control
                    let mut volume = *volume as isize;
                    let params = value.wrapping_add(if result.len() >= cutoff.unwrap() {
                        0
                    } else {
                        pitch
                    });

                    // apply this note
                    let shift = params >> 4;
                    let shift = if shift > 0xd { shift & 0xd } else { shift }; // not sure how to deal with E or F, but its so low you can hardly notice it anyway

                    let divider = params & 0x7;
                    let width = (params & 0x8) == 0x8;
                    let mut noise_buffer: u16 = 0x7fff;

                    for index in 0..2500000 {
                        if index >= sample_count && !(is_last_command && volume > 0) {
                            break;
                        }

                        let bit0 = noise_buffer & 1;
                        result.push(sample((1 ^ bit0) as isize, volume));

                        // according to params, update buffer
                        if result.len()
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
                        if *fade != 0
                            && ((index + 1) % (SAMPLES_PER_FRAME * (fade.unsigned_abs() as usize)))
                                == 0
                        {
                            volume += if *fade < 0 { 1 } else { -1 };
                            volume = volume.clamp(0, 0x0f);
                        }
                    }
                }
            }
        }

        result
    }
}
