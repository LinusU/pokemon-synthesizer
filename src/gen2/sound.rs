use super::channel::{
    Channel, ChannelIterator, ChannelType, SAMPLES_PER_FRAME, SOURCE_SAMPLE_RATE,
};

#[derive(Debug, Clone, Copy)]
pub struct Sound<'a> {
    pulse1: Option<Channel<'a>>,
    pulse2: Option<Channel<'a>>,
    wave: Option<Channel<'a>>,
    noise: Option<Channel<'a>>,
}

impl<'a> Sound<'a> {
    pub fn new(rom: &'a [u8], bank: u8, addr: u16) -> Sound<'a> {
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

    pub fn pcm(self, pitch: i16, length: u16) -> SoundIterator<'a> {
        SoundIterator::new(self, pitch, length)
    }
}

#[derive(Debug, Clone)]
pub struct SoundIterator<'a> {
    pulse1: Option<ChannelIterator<'a>>,
    pulse2: Option<ChannelIterator<'a>>,
    wave: Option<ChannelIterator<'a>>,
    noise: Option<ChannelIterator<'a>>,
    index: usize,
    buffer: [f32; SAMPLES_PER_FRAME],
}

impl<'a> SoundIterator<'a> {
    pub fn new(sound: Sound<'a>, pitch: i16, length: u16) -> SoundIterator<'a> {
        SoundIterator {
            pulse1: sound.pulse1.as_ref().map(|c| c.pcm(pitch, length)),
            pulse2: sound.pulse2.as_ref().map(|c| c.pcm(pitch, length)),
            wave: sound.wave.as_ref().map(|c| c.pcm(pitch, length)),
            noise: sound.noise.as_ref().map(|c| c.pcm(pitch, 0x100)),
            index: 0,
            buffer: [0.0; SAMPLES_PER_FRAME],
        }
    }

    pub fn channels(&self) -> u16 {
        1
    }

    pub fn sample_rate(&self) -> u32 {
        SOURCE_SAMPLE_RATE as u32
    }
}

impl<'a> Iterator for SoundIterator<'a> {
    type Item = f32;

    fn count(mut self) -> usize {
        let mut result = 0;

        loop {
            let mut done = true;

            if let Some(pulse1) = &mut self.pulse1 {
                if pulse1.next().is_some() {
                    done = false;
                }

                if pulse1.is_infinite() == Some(true) {
                    return usize::MAX;
                }
            }

            if let Some(pulse2) = &mut self.pulse2 {
                if pulse2.next().is_some() {
                    done = false;
                }

                if pulse2.is_infinite() == Some(true) {
                    return usize::MAX;
                }
            }

            if let Some(wave) = &mut self.wave {
                if wave.next().is_some() {
                    done = false;
                }

                if wave.is_infinite() == Some(true) {
                    return usize::MAX;
                }
            }

            if let Some(noise) = &mut self.noise {
                if noise.next().is_some() {
                    done = false;
                }

                if noise.is_infinite() == Some(true) {
                    return usize::MAX;
                }
            }

            if done {
                return result;
            }

            result += SAMPLES_PER_FRAME;
        }
    }

    fn next(&mut self) -> Option<f32> {
        if self.index % SAMPLES_PER_FRAME == 0 {
            self.buffer.fill(0.0);

            let mut done = true;

            if let Some(pulse1) = &mut self.pulse1 {
                if let Some(data) = pulse1.next() {
                    for (i, data) in data.iter().enumerate() {
                        self.buffer[i] += data / 3.0;
                    }

                    done = false;
                }
            }

            if let Some(pulse2) = &mut self.pulse2 {
                if let Some(data) = pulse2.next() {
                    for (i, data) in data.iter().enumerate() {
                        self.buffer[i] += data / 3.0;
                    }

                    done = false;
                }
            }

            if let Some(wave) = &mut self.wave {
                if let Some(data) = wave.next() {
                    for (i, data) in data.iter().enumerate() {
                        self.buffer[i] += data / 3.0;
                    }

                    done = false;
                }
            }

            if let Some(noise) = &mut self.noise {
                if let Some(data) = noise.next() {
                    for (i, data) in data.iter().enumerate() {
                        self.buffer[i] += data / 3.0;
                    }

                    done = false;
                }
            }

            if done {
                return None;
            }
        }

        let result = self.buffer[self.index % SAMPLES_PER_FRAME];
        self.index += 1;
        Some(result)
    }
}
