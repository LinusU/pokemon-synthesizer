use std::time::Duration;

use channel::SOURCE_SAMPLE_RATE;
use sound::Sound;

pub use sound::SoundIterator;

mod channel;
mod command;
mod sound;

#[derive(Debug, Clone)]
pub struct Pcm<'a> {
    pitch: i8,
    length: u16,
    sound: Sound<'a>,
}

impl<'a> Pcm<'a> {
    pub fn channels(&self) -> u16 {
        1
    }

    pub fn sample_rate(&self) -> u32 {
        SOURCE_SAMPLE_RATE as u32
    }

    pub fn total_duration(&self) -> Option<Duration> {
        let len = self.sound.pcm(self.pitch, self.length).count();

        if len == usize::MAX {
            None
        } else {
            Some(std::time::Duration::from_secs_f64(
                (len as f64) / (self.sample_rate() as f64),
            ))
        }
    }

    pub fn iter(&self) -> SoundIterator<'a> {
        self.sound.pcm(self.pitch, self.length)
    }
}

pub fn synthesis(rom: &[u8], bank: u8, addr: u16, pitch: i8, length: u8) -> Pcm {
    Pcm {
        sound: Sound::new(rom, bank, addr),
        pitch,
        length: (length as u16) + 0x80,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POKEYELLOW: &[u8] = include_bytes!("../../roms/pokeyellow.gbc");
    const WAVE_HEADER_LEN: usize = 44;

    fn convert_to_wav(input: &Pcm) -> Vec<u8> {
        assert_eq!(input.channels(), 1);

        let data: Vec<f32> = input.iter().collect();

        let resample_rate_ratio = input.sample_rate() as f64 / 48000.0;
        let resampled_length = (data.len() as f64 / resample_rate_ratio).ceil() as usize;
        let mut output = Vec::with_capacity(WAVE_HEADER_LEN + resampled_length);

        output.extend(b"RIFF");
        output.extend((resampled_length as u32).to_le_bytes());
        output.extend(b"WAVEfmt ");
        output.extend(16u32.to_le_bytes()); // remaining header size
        output.extend(1u16.to_le_bytes()); // PCM type
        output.extend(1u16.to_le_bytes()); // channels
        output.extend(48000u32.to_le_bytes()); // sample rate
        output.extend(48000u32.to_le_bytes()); // byte rate
        output.extend(1u16.to_le_bytes()); // block align
        output.extend(8u16.to_le_bytes()); // bits per sample
        output.extend(b"data");
        output.extend((resampled_length as u32).to_le_bytes());

        assert_eq!(output.len(), WAVE_HEADER_LEN);

        fn to_u8(value: f32) -> u8 {
            (value * 127.0 + 128.0) as u8
        }

        for resampled_index in 1..resampled_length {
            let prev_index = (resampled_index as f64 * resample_rate_ratio).floor() as usize;
            let next_index = (resampled_index as f64 * resample_rate_ratio).ceil() as usize;

            if prev_index == next_index {
                output.push(to_u8(data[prev_index]));
                continue;
            }

            let prev_fraction = resampled_index as f64 * resample_rate_ratio - prev_index as f64;
            let next_fraction = 1.0 - prev_fraction;

            output.push(to_u8(
                ((prev_fraction * (data[prev_index] as f64))
                    + (next_fraction * (data[next_index] as f64))) as f32,
            ));
        }

        output
    }

    fn assert_wav_almost_equal(actual: &[u8], expected: &[u8]) {
        assert_eq!(actual.len(), expected.len());

        assert_eq!(&actual[..WAVE_HEADER_LEN], &expected[..WAVE_HEADER_LEN],);

        for (index, (actual, expected)) in actual
            .iter()
            .skip(WAVE_HEADER_LEN)
            .zip(expected.iter().skip(WAVE_HEADER_LEN))
            .enumerate()
        {
            assert!(
                (*actual as i32 - *expected as i32).abs() <= 1,
                "actual: {actual}, expected: {expected}, at index: {index}",
            );
        }
    }

    #[test]
    fn test_bulbasaur_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x40c3, -128, 1);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../../expected/bulbasaur-cry.wav"),
        );
    }

    #[test]
    fn test_diglett_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x409f, -86, 1);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../../expected/diglett-cry.wav"),
        );
    }

    #[test]
    fn test_jigglypuff_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x40ba, -1, 53);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../../expected/jigglypuff-cry.wav"),
        );
    }

    #[test]
    fn test_snorlax_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x4069, 85, 1);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../../expected/snorlax-cry.wav"),
        );
    }

    #[test]
    fn test_aerodactyl_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x4177, 32, 240);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../../expected/aerodactyl-cry.wav"),
        );
    }

    #[test]
    fn test_pikachu_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x40c3, -18, 1);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../../expected/pikachu-cry.wav"),
        );
    }

    #[test]
    fn test_slowpoke_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x404e, 0, 128);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../../expected/slowpoke-cry.wav"),
        );
    }
}
