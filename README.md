garbage ai slop image viewer mainly for reading mango, do not download!!!111!!!!

## Build

Requires the Rust toolchain.

- Windows: `cargo build --release` (the .exe icon is embedded at build time;
  `installer/miru-install.bat` does the rest)
- macOS: `installer/macos/make-app.sh` builds a release binary and wraps it
  in a runnable `target/miru.app` bundle (icon generated at build time from
  the system CJK font; also works: plain `cargo build --release` to get the
  bare binary)
- Linux: `cargo build --release`

Config lives in `%APPDATA%\miru\config.toml` (Windows),
`~/Library/Application Support/miru/config.toml` (macOS),
`~/.config/miru/config.toml` (Linux).

