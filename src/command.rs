#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Command {
    DutyCycle(u8),
    DutyCyclePattern(u8, u8, u8, u8),
    SquareNote {
        length: u8,
        volume: u8,
        fade: i8,
        freq: u16,
    },
    NoiseNote {
        length: u8,
        volume: u8,
        fade: i8,
        value: u8,
    },
}
