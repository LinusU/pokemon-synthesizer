//! Renders a song from Gold, Silver or Crystal to a WAV file.
//!
//! cargo run --release --example music -- <rom> <bank> <addr> <seconds> <out.wav>
//!
//! Bank and address are of the song's header, in hex, such as `3a 7808`
//! for Crystal's title screen music.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, rom, bank, addr, seconds, out] = &args[..] else {
        eprintln!("usage: music <rom> <bank> <addr> <seconds> <out.wav>");
        std::process::exit(1);
    };

    let rom = std::fs::read(rom).unwrap();
    let bank = u8::from_str_radix(bank, 16).unwrap();
    let addr = u16::from_str_radix(addr, 16).unwrap();
    let seconds: f64 = seconds.parse().unwrap();

    const RATE: u32 = 48_000;
    let music =
        pokemon_synthesizer::gen2::music(&rom, bank, addr, RATE).expect("no sound engine in ROM");
    let samples: Vec<i16> = music
        .take((seconds * RATE as f64) as usize)
        .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect();

    let mut file = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
    let data_len = (samples.len() * 2) as u32;
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    file.write_all(&1u16.to_le_bytes()).unwrap(); // mono
    file.write_all(&RATE.to_le_bytes()).unwrap();
    file.write_all(&(RATE * 2).to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_len.to_le_bytes()).unwrap();
    for sample in samples {
        file.write_all(&sample.to_le_bytes()).unwrap();
    }
}
