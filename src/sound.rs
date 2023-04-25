use crate::channel::{Channel, ChannelType, SAMPLES_PER_FRAME};

#[derive(Debug)]
pub struct Sound {
    pulse1: Option<Channel>,
    pulse2: Option<Channel>,
    noise: Option<Channel>,
}

impl Sound {
    pub fn parse(rom: &[u8], bank: u8, addr: u16) -> Sound {
        let mut result = Sound {
            pulse1: None,
            pulse2: None,
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
                5 => assert!(result
                    .pulse1
                    .replace(Channel::parse(rom, bank, ptr, ChannelType::Pulse))
                    .is_none()),
                6 => assert!(result
                    .pulse2
                    .replace(Channel::parse(rom, bank, ptr, ChannelType::Pulse))
                    .is_none()),
                7 => todo!("Wave channel"),
                8 => assert!(result
                    .noise
                    .replace(Channel::parse(rom, bank, ptr, ChannelType::Noise))
                    .is_none()),
                _ => panic!("Invalid SFX channel: {}", id),
            }
        }

        result
    }

    pub fn pcm(&self, pitch: u8, length: i8) -> Vec<f32> {
        let mut result = Vec::new();

        let mut pulse1_len: usize = 0;
        if let Some(pulse1) = &self.pulse1 {
            for (index, sample) in pulse1.pcm(pitch, length, None).iter().enumerate() {
                if result.len() <= index {
                    result.push(sample / 3.0);
                } else {
                    result[index] += sample / 3.0;
                }
            }

            pulse1_len = pulse1.len(length);
        }

        let mut pulse2_len: usize = 0;
        if let Some(pulse2) = &self.pulse2 {
            for (index, sample) in pulse2.pcm(pitch, length, None).iter().enumerate() {
                if result.len() <= index {
                    result.push(sample / 3.0);
                } else {
                    result[index] += sample / 3.0;
                }
            }

            pulse2_len = pulse2.len(length);
        }

        // due to quirk with noise channel: find shortest channel length
        // at this point, noise will revert pitch shift effect

        let cutoff = usize::max(pulse1_len, pulse2_len) - SAMPLES_PER_FRAME;

        if let Some(noise) = &self.noise {
            for (index, sample) in noise.pcm(pitch, 0, Some(cutoff)).iter().enumerate() {
                if result.len() <= index {
                    result.push(sample / 3.0);
                } else {
                    result[index] += sample / 3.0;
                }
            }
        }

        result
    }
}
