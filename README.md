# Garou(がろう) - Simple Image Protocol Viewer for Kitty Graphics Protocol
```bash
siv.exe
```
**High-speed TUI image viewer using Kitty Graphics Protocol**

日本語 - [README-ja.md](README/README-ja.md)

## ⭐ Features
- **Fast Differential Display**: Optimized rendering for consecutive images
- **LRU Caching**: Memory-efficient image management
- **Debounce Control**: Optimized preview updates on cursor movement
- **Natural Sort**: Display file names in order 1,2,3,10,11,12

## 💻 System Requirements
### Terminal Emulator
- Must support **Kitty Graphics Protocol**
#### Verified
- [x] Wezterm Nightly
#### Not Working
- Windows Terminal

### Operating System
#### Verified
- [x] Windows 11 (64bit)
#### Unverified
- [ ] Linux
- [ ] Mac

## 📦 Installation

### cargo
#### `cargo install`
```bash
cargo install garou
```
#### `cargo binstall`
```bash
cargo binstall garou
```
#### `cargo install --git`
```bash
cargo install --git https://github.com/c0b23092db/garou
```

## 📖 Commands
```
> siv --help
TUI: Simple Image Protocol Viewer for Kitty Graphics Protocol

Usage: siv.exe [PATH]

Arguments:
  [PATH]  Open Image file or Directory [defaults: current directory]

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## ⌨️ Controls

### Always Available
- **`Q / esc`**: Quit
- **`O`**: Open with default program
- **`R`**: Refresh image
- **`Shift + R`**: Refresh screen
- **`Alt + S`**: Toggle sidebar
- **`Alt + D`**: Toggle statusbar
- **`Alt + F`**: Toggle header

### Image Display
- **`H / ←`**: Previous image
- **`L / →`**: Next image

### Sidebar
- **`J / ↓`**: Move cursor down (instant preview)
- **`K / ↑`**: Move cursor up (instant preview)
- **`G`**: Move to top (instant preview)
- **`Shift + G`**: Move to bottom (instant preview)
- **`Ctrl + B`**: Move up one page (instant preview)
- **`Ctrl + F`**: Move down one page (instant preview)
- **`H / ←`**: Collapse folder
- **`L / →`**: Expand folder
- **`Enter`**: Toggle folder
- **`Left Click`**: Select file
- **`Wheel`**: Move cursor

### Experimental / Test Controls (Preview)
- (preview) `0`: Fit image to view
- (preview) `+`: Zoom in
- (preview) `-`: Zoom out
- (preview) `Shift + J`, `Shift + K`, `Shift + H`, `Shift + L`: Pan image
- (preview) `Wheel`: Image Zoom in / out

## ⚙️ Configuration File
Reads from `~/.config/garou/config.toml`.

```toml
[image]
extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp"]
diff_mode = "Full"
transport_mode = "auto"
filter_type = "Nearest"
image_width = 5120
image_height = 2880
dirty_ratio = 0.1
tile_grid = 32
skip_step = 1

[display]
sidebar = true
header = true
statusbar = true
sidebar_size = 20
preview_debounce = 100       # Preview update debounce (milliseconds)
poll_interval = 10      # Idle polling interval (milliseconds)
prefetch_interval = 100 # Idle prefetch interval (milliseconds)
header_bg_color = "dark_blue"    # Header background color
header_fg_color = "white"        # Header text color
statusbar_bg_color = "dark_gray" # Statusbar background color
statusbar_fg_color = "white"     # Statusbar text color

[cache]
lru_size = 10         # Maximum LRU cache entries
prefetch_size = 1     # Prefetch cache size
max_bytes = 268435456 # Cache total size limit (bytes)
```

### **image**

#### Image display process (diff_mode)
- `All`: No differential check, refresh image every time
- `Full`: Check all RGB (FFFFFF) addresses, update only if changes detected
- `Half`: Check only RGB addresses 0, 2, 4

#### transport_mode (Kitty Graphics Protocol transfer mode)
- `auto`
- `direct` (`d`)
- `file` (`f`)
- `temp_file` (`t`)
- `shared_memory` (`s`)

##### auto behavior
- Linux: `shared_memory` -> `direct`
- Windows: `direct`

#### Maximum Image Size (image_width, image_height)
When using file, temp_file, or shared_memory, images larger than this size will fall back to direct mode.
Default allows loading images up to 5K resolution.

#### Decode Filter (filter_type)
Used when transport_mode = "direct"
See: [image::imageops::FilterType](https://docs.rs/image/latest/image/imageops/enum.FilterType.html)
- `Nearest`
- `Triangle`
- `CatmullRom`
- `Gaussian`
- `Lanczos3`

#### Differential determination threshold (dirty_ratio)
Threshold for determining if differential. Can be set from 0.0 to 1.0.

#### Differential determination tile (tile_grid)
Pixel size of one side of tiles used for change detection.

#### Pixel skipping (skip_step)
Scan at specified pixel intervals.

### **display**

#### Startup expansion
Settings for initial open/closed state.
- sidebar
- header
- statusbar

#### color
The following colors are available:
- black
- dark_gray
- gray
- white
- red
- dark_red
- green
- dark_green
- yellow
- dark_yellow
- blue
- dark_blue
- magenta
- dark_magenta
- cyan
- dark_cyan
- #RRGGBB (e.g., #1E90FF)
- rgb(r,g,b) (e.g., rgb(30,144,255))

#### Preview update debounce (preview_debounce)
Specifies the debounce time in milliseconds for preview updates during user operations.

#### Idle polling interval (poll_interval)
Specifies the maximum wait time in milliseconds before checking for keyboard or mouse activity during idle state.

#### Idle prefetch interval (prefetch_interval)
Specifies the minimum interval in milliseconds for prefetching adjacent images while the application is idle.

### **cache**

#### max_bytes
Default value is **268435456** (`256 * 1024 * 1024`).

## 🔭 Recommended Settings
### Windows (Local)
```toml
[image]
diff_mode = "All"
transport_mode = "file"

[display]
sidebar = true
header = true
statusbar = false
sidebar_size = 20
preview_debounce = 50

[cache]
lru_size = 5
prefetch_size = 1
```

## 📝 TODO
- Fast differential display
- Fast display for large size images

Please refer to the Japanese version of README.md for other details.

## 💡 Inspiration
- Kitty Graphics Protocol (terminal image rendering)
- yazi (high-speed image preview)
- Neovim (keyboard-driven operation)

## 🤝 Contributing
Bug reports, feature suggestions, and pull requests are welcome.
The agents directory includes details such as future prospects and plans.

## 📜 LICENSE
[MIT License](LICENSE) / <http://opensource.org/licenses/MIT>

## 👤 Developer
- ikata
