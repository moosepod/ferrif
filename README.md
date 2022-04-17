# ferrif

Rust-based Z-Machine interpreter that uses egui for a UI. 

## Features

- Supports ZCode versions 1,2,3
- Library-based design supporting IFiction format
- Autosaves and undo/redo 
- Built-in notes
- Support for Invisiclue-style hints
- Queztal-based saves

## Crates

- `ferrif` (this crate) is the UI for the interpreter
- `ferrif-zmachine` is an abstracted Z-Machine interpreter

## Notes

This interpreter will not play sound effects if the game contains them. A message will be printed to the screen instead.
