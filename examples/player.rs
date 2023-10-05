use pokemon_synthesizer::gen1::SoundIterator;
use rodio::{OutputStream, Source};

struct PcmSource<'a>(SoundIterator<'a>);

impl<'a> PcmSource<'a> {
    fn new(source: SoundIterator<'a>) -> PcmSource<'a> {
        PcmSource(source)
    }
}

impl Iterator for PcmSource<'_> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl Source for PcmSource<'_> {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.0.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
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

    let rom: &'static [u8] = Box::new(std::fs::read(rom_path).unwrap()).leak();

    let mut bank_addr = bank_addr.split(':');
    let bank: u8 = u8::from_str_radix(bank_addr.next().unwrap(), 16).unwrap();
    let addr: u16 = u16::from_str_radix(bank_addr.next().unwrap(), 16).unwrap();

    let pcm = pokemon_synthesizer::gen1::synthesis(rom, bank, addr, pitch, length);
    let duration = pcm.total_duration().unwrap_or(std::time::Duration::MAX);

    let (_stream, stream_handle) = OutputStream::try_default().unwrap();

    stream_handle.play_raw(PcmSource::new(pcm.iter())).unwrap();

    eprintln!("Playing for {:?}", duration);
    std::thread::sleep(duration);
}
