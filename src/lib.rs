use std::time::Duration;

use channel::SOURCE_SAMPLE_RATE;

mod channel;
mod command;
mod sound;

#[derive(Debug, Clone)]
pub struct Pcm {
    pub data: Vec<f32>,
}

impl Pcm {
    pub fn channels(&self) -> u16 {
        1
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }

    pub fn sample_rate(&self) -> u32 {
        SOURCE_SAMPLE_RATE as u32
    }

    pub fn total_duration(&self) -> Duration {
        std::time::Duration::from_secs_f64((self.data.len() as f64) / (self.sample_rate() as f64))
    }
}

pub fn synthesis(rom: &[u8], bank: u8, addr: u16, pitch: u8, length: i8) -> Pcm {
    Pcm {
        data: sound::Sound::new(rom, bank, addr).pcm(pitch, length).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POKEYELLOW: &[u8] = include_bytes!("../roms/pokeyellow.gbc");
    const WAVE_HEADER_LEN: usize = 44;

    fn convert_to_wav(input: &Pcm) -> Vec<u8> {
        assert_eq!(input.channels(), 1);

        let resample_rate_ratio = input.sample_rate() as f64 / 48000.0;
        let resampled_length = (input.data().len() as f64 / resample_rate_ratio).ceil() as usize;
        let mut output = Vec::with_capacity(WAVE_HEADER_LEN + resampled_length);

        output.extend(b"RIFF");
        output.extend(&(resampled_length as u32).to_le_bytes());
        output.extend(b"WAVEfmt ");
        output.extend(&16u32.to_le_bytes()); // remaining header size
        output.extend(&1u16.to_le_bytes()); // PCM type
        output.extend(&1u16.to_le_bytes()); // channels
        output.extend(&48000u32.to_le_bytes()); // sample rate
        output.extend(&48000u32.to_le_bytes()); // byte rate
        output.extend(&1u16.to_le_bytes()); // block align
        output.extend(&8u16.to_le_bytes()); // bits per sample
        output.extend(b"data");
        output.extend(&(resampled_length as u32).to_le_bytes());

        assert_eq!(output.len(), WAVE_HEADER_LEN);

        fn to_u8(value: f32) -> u8 {
            (value * 127.0 + 128.0) as u8
        }

        let data = input.data();

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

        for (actual, expected) in actual
            .iter()
            .skip(WAVE_HEADER_LEN)
            .zip(expected.iter().skip(WAVE_HEADER_LEN))
        {
            assert!(
                (*actual as i32 - *expected as i32).abs() <= 1,
                "actual: {}, expected: {}",
                actual,
                expected
            );
        }
    }

    #[test]
    fn test_bulbasaur_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x40c3, 128, -127);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../expected/bulbasaur-cry.wav"),
        );
    }

    #[test]
    fn test_diglett_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x409f, 170, -127);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../expected/diglett-cry.wav"),
        );
    }

    #[test]
    fn test_jigglypuff_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x40ba, 255, -75);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../expected/jigglypuff-cry.wav"),
        );
    }

    #[test]
    fn test_snorlax_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x4069, 85, -127);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../expected/snorlax-cry.wav"),
        );
    }

    #[test]
    fn test_aerodactyl_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x40b1, 32, 112);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../expected/aerodactyl-cry.wav"),
        );
    }

    #[test]
    fn test_pikachu_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x40c3, 238, -127);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../expected/pikachu-cry.wav"),
        );
    }

    #[test]
    fn test_slowpoke_cry() {
        let pcm = synthesis(POKEYELLOW, 0x02, 0x404e, 0, 0);

        assert_wav_almost_equal(
            &convert_to_wav(&pcm),
            include_bytes!("../expected/slowpoke-cry.wav"),
        );
    }
}
