use crate::channel::{Channel, ChannelType, ChannelIterator, SAMPLES_PER_FRAME};

#[derive(Debug)]
pub struct Sound<'a> {
    pulse1: Option<Channel<'a>>,
    pulse2: Option<Channel<'a>>,
    wave: Option<Channel<'a>>,
    noise: Option<Channel<'a>>,
}

impl Sound<'_> {
    pub fn new(rom: &[u8], bank: u8, addr: u16) -> Sound {
        let mut result = Sound {
            pulse1: None,
            pulse2: None,
            wave: None,
            noise: None,
        };

        let mut pos = ((bank as usize) * 0x4000) + ((addr as usize) & 0x3fff);
        let channel_count = (rom[pos] >> 6) + 1;

        for _ in 0..channel_count {
            let id = (rom[pos] & 0xf) + 1;
            pos += 1;

            let ptr = u16::from_le_bytes([rom[pos], rom[pos + 1]]);
            pos += 2;

            match id {
                1 => assert!(result
                    .pulse1
                    .replace(Channel::new(rom, bank, ptr, ChannelType::MusicPulse, 1))
                    .is_none()),
                2 => assert!(result
                    .pulse2
                    .replace(Channel::new(rom, bank, ptr, ChannelType::MusicPulse, 2))
                    .is_none()),
                3 => assert!(result
                    .wave
                    .replace(Channel::new(rom, bank, ptr, ChannelType::MusicWave, 3))
                    .is_none()),
                4 => assert!(result
                    .noise
                    .replace(Channel::new(rom, bank, ptr, ChannelType::MusicNoise, 4))
                    .is_none()),
                5 => assert!(result
                    .pulse1
                    .replace(Channel::new(rom, bank, ptr, ChannelType::SfxPulse, 5))
                    .is_none()),
                6 => assert!(result
                    .pulse2
                    .replace(Channel::new(rom, bank, ptr, ChannelType::SfxPulse, 6))
                    .is_none()),
                7 => assert!(result
                    .wave
                    .replace(Channel::new(rom, bank, ptr, ChannelType::SfxWave, 7))
                    .is_none()),
                8 => assert!(result
                    .noise
                    .replace(Channel::new(rom, bank, ptr, ChannelType::SfxNoise, 8))
                    .is_none()),
                _ => panic!("Invalid SFX channel: {}", id),
            }
        }

        result
    }

    pub fn pcm(&self, pitch: u8, length: i8) -> SoundIterator {
        SoundIterator::new(self, pitch, length)
    }
}

pub struct SoundIterator<'a> {
    pulse1: Option<ChannelIterator<'a>>,
    pulse2: Option<ChannelIterator<'a>>,
    wave: Option<ChannelIterator<'a>>,
    noise: Option<ChannelIterator<'a>>,
    index: usize,
    reset_pitch_at: usize,
    pitch_has_been_reset: bool,
}

impl<'a> SoundIterator<'a> {
    pub fn new(sound: &'a Sound<'a>, pitch: u8, length: i8) -> SoundIterator<'a> {
        let pulse1_len = sound.pulse1.as_ref().map(|c| c.len(length).unwrap()).unwrap();
        let pulse2_len = sound.pulse2.as_ref().map(|c| c.len(length).unwrap()).unwrap();

        dbg!(pulse1_len);
        dbg!(pulse2_len);

        SoundIterator {
            pulse1: sound.pulse1.as_ref().map(|c| c.pcm(pitch, length)),
            pulse2: sound.pulse2.as_ref().map(|c| c.pcm(pitch, length)),
            wave: sound.wave.as_ref().map(|c| c.pcm(pitch, length)),
            noise: sound.noise.as_ref().map(|c| c.pcm(pitch, length)),
            index: 0,
            reset_pitch_at: usize::max(pulse1_len, pulse2_len) - SAMPLES_PER_FRAME,
            pitch_has_been_reset: false,
        }
    }
}

impl<'a> Iterator for SoundIterator<'a> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // FIXME: Set pitch of noise channel to 0 when pulse channels are done (minus the fade out of the last note)

        let mut result = 0.0;
        let mut done = true;
        let mut almost_done = true;

        if let Some(pulse1) = &mut self.pulse1 {
            if let Some(sample) = pulse1.next() {
                result += sample / 3.0;
                done = false;

                if !pulse1.only_fadeout_left() {
                    almost_done = false;
                }
            }
        }

        if let Some(pulse2) = &mut self.pulse2 {
            if let Some(sample) = pulse2.next() {
                result += sample / 3.0;
                done = false;

                if !pulse2.only_fadeout_left() {
                    almost_done = false;
                }
            }
        }

        if let Some(wave) = &mut self.wave {
            if let Some(sample) = wave.next() {
                result += sample / 3.0;
                done = false;
            }
        }

        if (self.index - 1) == self.reset_pitch_at {
            eprintln!("almost_done should be false at index {}, is {}", self.index, almost_done);
        }

        if self.index == self.reset_pitch_at {
            eprintln!("almost_done should be true at index {}, is {}", self.index, almost_done);
        }

        if !self.pitch_has_been_reset && almost_done {
            eprintln!("almost_done was true at index {}, should be at index {}", self.index, self.reset_pitch_at);
            self.pitch_has_been_reset = true;
        }

        if let Some(noise) = &mut self.noise {
            // if almost_done && !self.pitch_has_been_reset {
            //     // eprintln!("Resetting noise pitch");
            //     noise.reset_pitch();
            //     self.pitch_has_been_reset = true;
            // }

            if self.index == self.reset_pitch_at {
                eprintln!("Resetting noise pitch");
                noise.reset_pitch();
            }

            if let Some(sample) = noise.next() {
                result += sample / 3.0;
                done = false;
            }
        }

        self.index += 1;

        if done { None } else { Some(result) }
    }
}


// #[cfg(test)]
// mod test {
//     use super::*;

//     const POKEYELLOW: &[u8] = include_bytes!("../roms/pokeyellow.gbc");

//     #[test]
//     fn parse_bank_02_sounds() {
//         for addr in (0x4003u16..=0x42f1u16).step_by(3) {
//             let sound = Sound::new(POKEYELLOW, 0x02, addr);
//             if let Some(pulse1) = sound.pulse1 {
//                 pulse1.
//             }
//         }
//     }

//     #[test]
//     fn parse_bank_08_sounds() {
//         for addr in (0x4003u16..=0x42f4u16).step_by(3) {
//             let sound = Sound::new(POKEYELLOW, 0x08, addr);
//             if let Some(pulse1) = sound.pulse1 {
//                 pulse1.
//             }
//         }
//     }

//     #[test]
//     fn parse_bank_1f_sounds() {
//         for addr in (0x4003u16..=0x42f1u16).step_by(3) {
//             let sound = Sound::new(POKEYELLOW, 0x1f, addr);
//             if let Some(pulse1) = sound.pulse1 {
//                 pulse1.
//             }
//         }
//     }

//     #[test]
//     fn parse_bank_20_sounds() {
//         for addr in (0x4003u16..=0x41e9u16).step_by(3) {
//             let sound = Sound::new(POKEYELLOW, 0x20, addr);
//             if let Some(pulse1) = sound.pulse1 {
//                 pulse1.
//             }
//         }
//     }
// }
