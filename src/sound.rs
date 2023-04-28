use crate::channel::{Channel, ChannelType, ChannelIterator};

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
                    .replace(Channel::new(rom, bank, ptr, ChannelType::MusicPulse))
                    .is_none()),
                2 => assert!(result
                    .pulse2
                    .replace(Channel::new(rom, bank, ptr, ChannelType::MusicPulse))
                    .is_none()),
                3 => assert!(result
                    .wave
                    .replace(Channel::new(rom, bank, ptr, ChannelType::MusicWave))
                    .is_none()),
                4 => assert!(result
                    .noise
                    .replace(Channel::new(rom, bank, ptr, ChannelType::MusicNoise))
                    .is_none()),
                5 => assert!(result
                    .pulse1
                    .replace(Channel::new(rom, bank, ptr, ChannelType::SfxPulse))
                    .is_none()),
                6 => assert!(result
                    .pulse2
                    .replace(Channel::new(rom, bank, ptr, ChannelType::SfxPulse))
                    .is_none()),
                7 => assert!(result
                    .wave
                    .replace(Channel::new(rom, bank, ptr, ChannelType::SfxWave))
                    .is_none()),
                8 => assert!(result
                    .noise
                    .replace(Channel::new(rom, bank, ptr, ChannelType::SfxNoise))
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
}

impl<'a> SoundIterator<'a> {
    pub fn new(sound: &'a Sound<'a>, pitch: u8, length: i8) -> SoundIterator<'a> {
        SoundIterator {
            pulse1: sound.pulse1.as_ref().map(|c| c.pcm(pitch, length)),
            pulse2: sound.pulse2.as_ref().map(|c| c.pcm(pitch, length)),
            wave: sound.wave.as_ref().map(|c| c.pcm(pitch, length)),
            noise: sound.noise.as_ref().map(|c| c.pcm(pitch, length)),
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

        if let Some(noise) = &mut self.noise {
            if almost_done {
                // eprintln!("Resetting noise pitch");
                noise.reset_pitch();
            }

            if let Some(sample) = noise.next() {
                result += sample / 3.0;
                done = false;
            }
        }

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
