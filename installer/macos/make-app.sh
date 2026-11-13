#!/bin/sh
# Assembles a runnable macOS app bundle: target/miru.app
# (builds the release binary, generates the .icns from the 見 glyph,
# copies in Info.plist and ad-hoc signs the result.)
set -eu
cd "$(dirname "$0")/../.."

# Locate cargo even if it is not on PATH (homebrew rustup is keg-only).
if ! command -v cargo >/dev/null 2>&1; then
    for dir in \
        "$(command -v brew >/dev/null 2>&1 && brew --prefix rustup)/bin" \
        "$HOME/.cargo/bin" \
        /opt/homebrew/opt/rustup/bin \
        /usr/local/opt/rustup/bin \
        "$HOME"/.rustup/toolchains/*/bin; do
        if [ -x "$dir/cargo" ]; then
            PATH="$dir:$PATH"
            break
        fi
    done
fi
command -v cargo >/dev/null 2>&1 || {
    echo "error: cargo not found; install Rust via https://rustup.rs" >&2
    exit 1
}

cargo build --release
cargo run --release --bin make-icon -- target/miru.icns

APP=target/miru.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp target/release/miru "$APP/Contents/MacOS/miru"
cp target/miru.icns "$APP/Contents/Resources/miru.icns"
cp installer/macos/Info.plist "$APP/Contents/Info.plist"
rm target/miru.icns

if command -v codesign >/dev/null 2>&1; then
    codesign --force -s - "$APP"
fi

echo "built $APP"
