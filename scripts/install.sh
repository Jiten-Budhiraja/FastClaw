#!/bin/bash
# Fastclaw One-Click Install Script for macOS
# Downloads the binary, installs dependencies, and launches the VM.

set -e

# Configuration
REPO="RomanSurface/FastClaw"
BINARY_NAME="fastclaw"
INSTALL_DIR="/usr/local/bin"
VM="1"

echo "🚀 Installing Fastclaw..."

# Detect Architecture
ARCH=$(uname -m)
case "$ARCH" in
    arm64)
        TARGET="aarch64-apple-darwin"
        ;;
    x86_64)
        TARGET="x86_64-apple-darwin"
        ;;
    *)
        echo "❌ Error: Fastclaw is only supported on macOS (ARM64 or x86_64)."
        exit 1
        ;;
esac

echo "📦 Detected architecture: $ARCH ($TARGET)"

# ── Step 1: Download and install Fastclaw binary ──

echo "🔍 Finding latest version..."
LATEST_TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo "❌ Error: Could not find latest release on GitHub."
    exit 1
fi

echo "📥 Downloading version $LATEST_TAG..."
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/fastclaw-$TARGET.tar.gz"
TEMP_DIR=$(mktemp -d)
curl -L -o "$TEMP_DIR/fastclaw.tar.gz" "$DOWNLOAD_URL"

echo "📦 Extracting..."
tar -xzf "$TEMP_DIR/fastclaw.tar.gz" -C "$TEMP_DIR"

echo "🛡️ Installing to $INSTALL_DIR (may ask for password)..."
sudo mv "$TEMP_DIR/fastclaw-$TARGET" "$INSTALL_DIR/$BINARY_NAME"
sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo "🔓 Removing macOS quarantine flag..."
sudo xattr -d com.apple.quarantine "$INSTALL_DIR/$BINARY_NAME" 2>/dev/null || true

rm -rf "$TEMP_DIR"

echo "✅ Fastclaw binary installed."
echo ""

# ── Step 2: Install Homebrew (if missing) ──

if ! command -v brew &>/dev/null; then
    echo "🍺 Homebrew not found. Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    # Make brew available in current session
    if [ -f /opt/homebrew/bin/brew ]; then
        eval "$(/opt/homebrew/bin/brew shellenv)"
    elif [ -f /usr/local/bin/brew ]; then
        eval "$(/usr/local/bin/brew shellenv)"
    fi
    echo "✅ Homebrew installed."
else
    echo "✅ Homebrew already installed."
fi

# ── Step 3: Install Tart (if missing) ──

if ! command -v tart &>/dev/null; then
    echo "📦 Installing Tart (VM runtime)..."
    brew install cirruslabs/cli/tart
    echo "✅ Tart installed."
else
    echo "✅ Tart already installed."
fi

# ── Step 4: Pull base image ──

echo ""
echo "📥 Pulling base Debian image (this may take a few minutes)..."
fastclaw image pull

# ── Step 5: Create and provision the VM ──

echo ""
echo "=== Creating and provisioning VM (~8-12 min) ==="
fastclaw up --number "$VM"

echo ""
echo "⏳ Waiting for VM to reboot into XFCE desktop (~30s)..."
sleep 30

echo ""
echo "🎉 Done! OpenClaw is ready."
echo "   The XFCE desktop is open in the Tart window."
echo ""
echo "   Useful commands:"
echo "     fastclaw shell $VM    — SSH into the VM"
echo "     fastclaw down $VM     — Stop the VM"
echo "     fastclaw up            — Start it again"
