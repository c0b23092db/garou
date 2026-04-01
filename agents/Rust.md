---
name: Rust Programming
description: Support for Rust Coding
---

# Rust
Last Updata: 2026/03/14 2:00

## Coding Rule
- Rust Style Guide

## Template Directory
```
 .
├─  LICENSE
├─  README.md
├─  Cargo.toml
├─  .gitignore
├─  .git: git project
├─  .jj: jj project
├─  target: target directory
├─  assets: assets directory
├─  benches: benchmarks directory
├─  examples: examples directory
└─ 󱧼 src
   ├─  main.rs
   ├─  lib.rs
   ├─  __about__.rs: pub const CARGO_PKG_VERSION etc...
   ├─ 󰙨 tests: Test logic
   ├─  core: Business logic
   ├─  utils: Function logic
   ├─  models: struct,trait,impl
   ├─  cli: Command Line Interface
   ├─  cui: Character User Interface
   ├─  tui: Text User Interface
   └─  gui: Graphical User Interface
```

## Use Command
- `cargo fmt`: Auto format
- `cargo check`: Static confirmation
- `cargo test`: Running tests
- `cargo run`: Running binary
- `cargo build --release`: Building release binary
