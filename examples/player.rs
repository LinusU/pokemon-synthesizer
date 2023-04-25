use pokemon_synthesizer::Pcm;
use rodio::{OutputStream, Source};

struct PcmSource {
    pcm: Pcm,
    pos: usize,
}

impl PcmSource {
    fn new(pcm: Pcm) -> Self {
        Self { pcm, pos: 0 }
    }
}

impl Iterator for PcmSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let data = self.pcm.data();

        if self.pos < data.len() {
            let result = data[self.pos];
            self.pos += 1;
            Some(result)
        } else {
            None
        }
    }
}

impl Source for PcmSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.pcm.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.pcm.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        Some(self.pcm.total_duration())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 5 {
        eprintln!("Usage: player <rom_file_path> <bank:addr> <pitch> <length>");
        std::process::exit(1);
    }

    let rom_path = &args[1];
    let bank_addr = &args[2];
    let pitch: u8 = args[3].parse().unwrap();
    let length: i8 = args[4].parse().unwrap();

    let rom = std::fs::read(rom_path).unwrap();

    let mut bank_addr = bank_addr.split(":");
    let bank: u8 = u8::from_str_radix(bank_addr.next().unwrap(), 16).unwrap();
    let addr: u16 = u16::from_str_radix(bank_addr.next().unwrap(), 16).unwrap();

    let pcm = pokemon_synthesizer::synthesis(&rom, bank, addr, pitch, length);
    let duration = pcm.total_duration();

    let (_stream, stream_handle) = OutputStream::try_default().unwrap();

    stream_handle.play_raw(PcmSource::new(pcm)).unwrap();

    eprintln!("Playing for {:?}", duration);
    std::thread::sleep(duration);
}
