use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 6 {
        eprintln!("Usage: export <rom_file_path> <bank:addr> <pitch> <length> <out_file_path>");
        std::process::exit(1);
    }

    let rom_path = &args[1];
    let bank_addr = &args[2];
    let pitch: i8 = args[3].parse().unwrap();
    let length: u8 = args[4].parse().unwrap();

    let rom: &'static [u8] = Box::new(std::fs::read(rom_path).unwrap()).leak();

    let mut bank_addr = bank_addr.split(':');
    let bank: u8 = u8::from_str_radix(bank_addr.next().unwrap(), 16).unwrap();
    let addr: u16 = u16::from_str_radix(bank_addr.next().unwrap(), 16).unwrap();

    let input = pokemon_synthesizer::gen1::synthesis(rom, bank, addr, pitch, length);
    let mut input_len = input.iter().count();

    if input_len == usize::MAX {
        eprintln!("Warning: source is infinitly long, exporting 1 minute of data");
        input_len = (input.sample_rate() as usize) * 60;
    }

    eprintln!(
        "Exporting {:?} of data",
        std::time::Duration::from_secs_f64((input_len as f64) / (input.sample_rate() as f64))
    );

    let resample_rate_ratio = input.sample_rate() as f64 / 48000.0;
    let resampled_length = (input_len as f64 / resample_rate_ratio).ceil() as usize;

    let mut file = std::fs::File::create(&args[5]).unwrap();

    file.write(b"RIFF").unwrap();
    file.write(&(resampled_length as u32).to_le_bytes())
        .unwrap();
    file.write(b"WAVEfmt ").unwrap();
    file.write(&16u32.to_le_bytes()).unwrap(); // remaining header size
    file.write(&1u16.to_le_bytes()).unwrap(); // PCM type
    file.write(&1u16.to_le_bytes()).unwrap(); // channels
    file.write(&48000u32.to_le_bytes()).unwrap(); // sample rate
    file.write(&48000u32.to_le_bytes()).unwrap(); // byte rate
    file.write(&1u16.to_le_bytes()).unwrap(); // block align
    file.write(&8u16.to_le_bytes()).unwrap(); // bits per sample
    file.write(b"data").unwrap();
    file.write(&(resampled_length as u32).to_le_bytes())
        .unwrap();

    fn to_u8(value: f32) -> u8 {
        (value * 127.0 + 128.0) as u8
    }

    let data: Vec<f32> = input.iter().take(input_len).collect();

    for resampled_index in 1..resampled_length {
        let prev_index = (resampled_index as f64 * resample_rate_ratio).floor() as usize;
        let next_index = (resampled_index as f64 * resample_rate_ratio).ceil() as usize;

        if prev_index == next_index {
            file.write(&[to_u8(data[prev_index])]).unwrap();
            continue;
        }

        let prev_fraction = resampled_index as f64 * resample_rate_ratio - prev_index as f64;
        let next_fraction = 1.0 - prev_fraction;

        file.write(&[to_u8(
            ((prev_fraction * (data[prev_index] as f64))
                + (next_fraction * (data[next_index] as f64))) as f32,
        )])
        .unwrap();
    }
}
