#!/usr/bin/env bash
# cross_compile_rpi.sh — Cross-compile HELIOS for Raspberry Pi (aarch64 Linux)
#
# Prerequisites (Ubuntu/Debian host):
#   sudo apt install gcc-aarch64-linux-gnu
#   rustup target add aarch64-unknown-linux-gnu

set -euo pipefail

TARGET="aarch64-unknown-linux-gnu"
LINKER="aarch64-linux-gnu-gcc"

echo "=== HELIOS cross-compile for Raspberry Pi ==="
echo "Target : $TARGET"
echo "Linker : $LINKER"
echo

# 1. Ensure the Rust target is installed
echo "[1/3] Installing Rust target..."
rustup target add "$TARGET"

# 2. Set the cross-linker via environment variable
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$LINKER"

# 3. Build from the rust/ subdirectory
cd "$(dirname "$0")/../rust"
echo "[2/3] Running cargo build --release --target $TARGET"
cargo build --release --target "$TARGET"

BINARY="target/$TARGET/release/helios-core"
echo
echo "[3/3] Build complete:"
ls -lh "$BINARY"
echo
echo "Copy to RPi with:"
echo "  scp $BINARY pi@<RPI_IP>:~/helios-core"
