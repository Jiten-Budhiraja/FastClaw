#!/bin/bash
# deploy.sh — Delete existing VM and recreate from scratch.
# Run AFTER cargo build --release, from the fastclaw/ directory.
# Usage: ./deploy.sh [vm_number]

set -e

VM="${1:-1}"
FASTCLAW="./target/release/fastclaw"

if [ ! -f "$FASTCLAW" ]; then
  echo "Error: binary not found. Run first: cargo build --release"
  exit 1
fi

echo "=== Cleaning up VM fastclaw-$VM ==="
STATE_FILE="$HOME/.fastclaw/state/fastclaw-$VM.json"
if [ -f "$STATE_FILE" ]; then
  rm -f "$STATE_FILE"
  echo "State file removed."
fi
"$FASTCLAW" delete "$VM" 2>/dev/null && echo "VM deleted." || echo "(no VM to delete)"

echo ""
echo "=== Creating and provisioning VM (~8-12 min) ==="
"$FASTCLAW" up --number "$VM"

echo ""
echo "Waiting for VM to reboot into XFCE (~30s)..."
sleep 30

echo ""
echo "✓ Ready! XFCE desktop is open in the Tart window."
echo "  SSH:  $FASTCLAW shell $VM"
echo "  Stop: $FASTCLAW down $VM"
