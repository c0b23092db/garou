---
name: Commands & Tools
description: Available Command And Prohibited Commands
---

# Commands & Tools
Last Updata: 2026/03/14 2:00

## When to Use skills
- When AGENTS want to know the **Available Command And Prohibited Commands**

## Environment Requirements

### Windows 11 Pro
- mise: latest
  - Rust: latest
  - uv: latest
  - Python: 3.13.10
  - clang: 21
  - zig: latest
  - Node.js: 24
  - deno: latest

### Pop!\_OS 24 LTS
- mise: latest
  - Rust: latest
  - uv: latest
  - Python: 3.13.10
  - clang: 21
  - zig: latest

## Available Command

### winget
- Git.Git
- Gyan.FFmpeg
- Visual Studio Build Tool

### mise
Use Command for Windows: `mise ls | ForEach-Object { ($_ -split '\s+')[0] } | Where-Object { $_ -ne 'Tool' } | ForEach-Object { '- ' + $_ }`
- bat
- cargo-binstall
- cargo:cargo-update
- cargo:diffx
- cargo:pastel
- cargo:tabiew
- clang
- cmake
- deno
- dotnet
- edit
- fd
- fzf
- gitui
- hyperfine
- java
- jj
- jq
- lazydocker
- lazygit
- lsd
- neovim
- node
- python
- ripgrep
- rust
- uv
- yazi
- yt-dlp
- zig
- zoxide

### cargo
Use Command for Windows: `cargo install --list | Where-Object { $_ -match '^[^\s]' } | ForEach-Object { ($_ -split ':')[0] } | ForEach-Object { '- ' + $_ }`
- Neiro v0.2.1
- cargo-about v0.8.4
- cargo-generate v0.23.7
- cargo-license v0.7.0
- create-tauri-app v4.7.0
- dioxus-cli v0.7.3
- dotter v0.13.4
- download_mover v1.1.2
- genact v1.5.1
- rhiza v0.1.5
- suppa v0.2.0
- tauri-cli v2.10.1

## Prohibited Commands
- `curl`
- `chmod 777`
- `rm -rf ~/`
- `rm -rf ~`
- `rm -rf ~/*`
- `rm -rf /*`
