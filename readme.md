# Pokemon Synthesizer

A synthesizer for the sound format of the Pokemon GameBoy games.

Implementation status:

- [x] Gen 1
    - [x] Pulse channels
    - [x] Noise channels
    - [ ] Wave channels
    - [x] Finite loops
    - [x] Infinite loops
    - [x] Pitch sweeps
    - [ ] Music sequences
- [x] Gen 2
    - [x] Pulse channels
    - [x] Noise channels
    - [ ] Wave channels
    - [x] Finite loops
    - [x] Infinite loops
    - [x] Pitch sweeps
    - [ ] Music sequences
- [ ] Future generations

## Installation

```sh
cargo add pokemon-synthesizer
```

## Usage

```rust
const rom = std::fs::read("pokeyellow.gbc").unwrap();

// Pikachu cry
let pcm = pokemon_synthesizer::gen1::synthesis(&rom, 0x02, 0x40c3, 238, -127);

// 1
pcm.channels()

// 1_048_576
pcm.sample_rate()

// Duration { 987.819672ms }
pcm.total_duration()

// Iterator<Item = f32>
pcm.iter()
```

## Testing

In order to run the `gen1` tests, you need a ROM file for Pokemon Yellow. The ROM file should have the SHA1 hash `cc7d03262ebfaf2f06772c1a480c7d9d5f4a38e1` and be named `roms/pokeyellow.gbc`.

In order to run the `gen2` tests, you need a ROM file for Pokemon Crystal. The ROM file should have the SHA1 hash `f4cd194bdee0d04ca4eac29e09b8e4e9d818c133` and be named `roms/pokecrystal.gbc`.

## Acknowledgements

Huge thanks to [Retro Game Mechanics Explained](https://www.youtube.com/channel/UCwRqWnW5ZkVaP_lZF7caZ-g) for their [video on Pokemon cries](https://www.youtube.com/watch?v=gDLpbFXnpeY), and for the accompanying tool. I would not have been able to make this without their help.

Also a big thanks to [the pret team](https://github.com/orgs/pret/people) for their [disassembly of Pokemon Yellow](https://github.com/pret/pokeyellow) which helped immensely in understanding the sound engine.
