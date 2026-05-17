#!/usr/bin/env bash
# deploy_rpi.sh — Deploy HELIOS to Raspberry Pi
#
# Usage:
#   RPI=pi@192.168.1.100 bash scripts/deploy_rpi.sh
#   or set RPI in your shell environment before running.

set -euo pipefail

RPI="${RPI:-pi@raspberrypi.local}"
TARGET="aarch64-unknown-linux-gnu"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$REPO_ROOT/rust/target/$TARGET/release/helios-core"
REMOTE_DIR="/home/pi/helios"

echo "=== HELIOS deploy to $RPI ==="
echo

if [ ! -f "$BINARY" ]; then
    echo "[ERR] Binary not found at $BINARY"
    echo "      Run scripts/cross_compile_rpi.sh first."
    exit 1
fi

# 1. Create remote directory structure
echo "[1/4] Creating remote directories..."
ssh "$RPI" "mkdir -p $REMOTE_DIR/data $REMOTE_DIR/ai"

# 2. Copy the compiled binary
echo "[2/4] Copying binary..."
scp "$BINARY" "$RPI:$REMOTE_DIR/helios-core"
ssh "$RPI" "chmod +x $REMOTE_DIR/helios-core"

# 3. Copy config and PVGIS data
echo "[3/4] Copying config and data..."
scp "$REPO_ROOT/helios_config.toml"         "$RPI:$REMOTE_DIR/"
scp "$REPO_ROOT/data/pvgis_murcia_tmy.csv"  "$RPI:$REMOTE_DIR/data/"
# Copy trained model if present
if [ -f "$REPO_ROOT/data/helios_predictor.pt" ]; then
    scp "$REPO_ROOT/data/helios_predictor.pt" "$RPI:$REMOTE_DIR/data/"
fi

# 4. Copy AI agent
echo "[4/4] Copying AI agent..."
scp "$REPO_ROOT/ai/agent.py"       "$RPI:$REMOTE_DIR/ai/"
scp "$REPO_ROOT/requirements.txt"  "$RPI:$REMOTE_DIR/"

echo
echo "=== Deploy complete ==="
echo
echo "On the RPi run:"
echo "  cd $REMOTE_DIR"
echo "  pip install -r requirements.txt"
echo "  python ai/agent.py --serve &"
echo "  ./helios-core"
echo
echo "For hardware mode, edit helios_config.toml:"
echo "  mode = \"hardware\""
