# Rusty Clicker (Rust / egui)

# Building

Build with: `cargo build --release`

The target folder for the release build: `target/release/rustyclicker`

# Configuration

- Profiles are stored in `<config dir>/rustyclicker/config.json`
  (Windows: %APPDATA%, macOS: ~/Library/Application Support, Linux: ~/.config).
- Last saved/selected profile is restored on start.
- Global start/stop hotkey (default F6) configurable via "Hotkey setting".

# Platform notes
- **Linux:** needs X11 (or XWayland apps). Pure Wayland blocks global hotkeys and synthetic clicks.
- **macOS:** grant Accessibility permission on first run.

# Screenshot

![App](docs\app.png)