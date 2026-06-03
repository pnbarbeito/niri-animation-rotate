#!/usr/bin/env bash
#
# install.sh — niri-animation-rotate installer
#
# Builds the binary, copies it to ~/.local/bin/, sets up config directories,
# copies bundled animations, and optionally installs a systemd user service.
#
# Usage:
#   ./install.sh                  # Install from local checkout
#   ./install.sh --systemd        # Install with systemd service
#   ./install.sh --help           # Show help
#
# You can also run this directly from the repo after cloning:
#   git clone https://github.com/pnbarbeito/niri-animation-rotate.git
#   cd niri-animation-rotate
#   ./install.sh
#
# Or in one shot (after inspecting the script):
#   curl -fsSL https://raw.githubusercontent.com/pnbarbeito/niri-animation-rotate/main/install.sh | bash
#

set -euo pipefail

# ──────────────────────────────────────────────
# Configuration
# ──────────────────────────────────────────────
BIN_NAME="niri-animation-rotate"
REPO_URL="https://github.com/pnbarbeito/niri-animation-rotate.git"

XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
CONFIG_DIR="$XDG_CONFIG_HOME/niri/niri-animation-rotate"
ANIMATIONS_DIR="$CONFIG_DIR/animations"
BIN_DIR="${HOME}/.local/bin"
SERVICE_DIR="${XDG_CONFIG_HOME}/systemd/user"
SERVICE_NAME="niri-animation-rotate.service"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# ──────────────────────────────────────────────
# Functions
# ──────────────────────────────────────────────
info()  { echo -e "${GREEN}✓${NC} $1"; }
warn()  { echo -e "${YELLOW}⚠${NC} $1"; }
error() { echo -e "${RED}✗${NC} $1"; }
header(){ echo -e "\n${CYAN}━━━ $1 ━━━${NC}\n"; }

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Options:
  --systemd    Install and enable a systemd user service
  --help       Show this help message and exit

Environment:
  CARGO_HOME  Path to Cargo installation (default: ~/.cargo)

The installer will:
  1. Check for required tools (Rust/Cargo, Git)
  2. Build niri-animation-rotate from source
  3. Copy the binary to ~/.local/bin/
  4. Create config directories under ~/.config/niri/niri-animation-rotate/
  5. Copy bundled animations (27 presets) to the config directory
  6. Optionally set up a systemd user service (with --systemd)
  7. Print next steps for Niri configuration
EOF
    exit 0
}

# ──────────────────────────────────────────────
# Parse arguments
# ──────────────────────────────────────────────
INSTALL_SYSTEMD=false
for arg in "$@"; do
    case "$arg" in
        --help) usage ;;
        --systemd) INSTALL_SYSTEMD=true ;;
        *)
            error "Unknown option: $arg"
            echo "Use --help for usage information."
            exit 1
            ;;
    esac
done

# ──────────────────────────────────────────────
# Step 0: Header
# ──────────────────────────────────────────────
echo -e "${CYAN}"
echo "╔══════════════════════════════════════════╗"
echo "║       niri-animation-rotate installer    ║"
echo "╚══════════════════════════════════════════╝"
echo -e "${NC}"

# ──────────────────────────────────────────────
# Step 1: Check prerequisites
# ──────────────────────────────────────────────
header "1/6 — Checking prerequisites"

# Check if we're in the repo directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
IN_REPO=false
if [ -f "$SCRIPT_DIR/Cargo.toml" ] && grep -q 'name = "niri-animation-rotate"' "$SCRIPT_DIR/Cargo.toml" 2>/dev/null; then
    IN_REPO=true
    REPO_DIR="$SCRIPT_DIR"
    info "Found local repo at $REPO_DIR"
fi

# Check for Rust/Cargo
if command -v cargo &>/dev/null; then
    info "Rust/Cargo found: $(cargo --version | head -1)"
else
    warn "Rust/Cargo not found."
    echo ""
    echo "  Install the Rust toolchain with:"
    echo ""
    echo "      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo ""
    echo "  Then restart your shell and run this installer again."
    echo ""
    exit 1
fi

# Check for Git (only needed if not in repo)
if ! $IN_REPO && ! command -v git &>/dev/null; then
    error "Git is required to clone the repository. Install git and try again."
    exit 1
fi

# ──────────────────────────────────────────────
# Step 2: Get the source code
# ──────────────────────────────────────────────
header "2/6 — Getting source code"

if $IN_REPO; then
    info "Using local repository"
    cd "$REPO_DIR"
else
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT
    info "Cloning repository from $REPO_URL"
    git clone --depth=1 "$REPO_URL" "$TMP_DIR" || {
        error "Failed to clone repository."
        exit 1
    }
    cd "$TMP_DIR"
    REPO_DIR="$TMP_DIR"
    info "Repository cloned"
fi

# ──────────────────────────────────────────────
# Step 3: Build
# ──────────────────────────────────────────────
header "3/6 — Building binary"

info "Running cargo build --release (this may take a few minutes)..."
if cargo build --release; then
    info "Build successful!"
else
    error "Build failed. Please check the output above for errors."
    exit 1
fi

# ──────────────────────────────────────────────
# Step 4: Install binary
# ──────────────────────────────────────────────
header "4/6 — Installing binary"

# Stop any running instance so the old binary can be safely replaced
if $INSTALL_SYSTEMD; then
    systemctl --user stop "$SERVICE_NAME" 2>/dev/null || true
fi

mkdir -p "$BIN_DIR"
rm -f "$BIN_DIR/$BIN_NAME"     # allow overwriting even if daemon is running
cp "target/release/$BIN_NAME" "$BIN_DIR/"
info "Binary installed to $BIN_DIR/$BIN_NAME"

# Install nrctl control script (optional, only if present in checkout)
if [ -f "nrctl" ]; then
    cp "nrctl" "$BIN_DIR/"
    chmod +x "$BIN_DIR/nrctl"
    info "Control script installed: $BIN_DIR/nrctl"
else
    warn "nrctl control script not found — skipping"
fi

# Check if BIN_DIR is in PATH
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    warn "$BIN_DIR is not in your PATH."
    echo "  Add it to your shell config (~/.bashrc, ~/.zshrc, etc.):"
    echo ""
    echo "      export PATH=\"\$PATH:$BIN_DIR\""
    echo ""
fi

# ──────────────────────────────────────────────
# Step 5: Setup config directories and animations
# ──────────────────────────────────────────────
header "5/6 — Setting up animations"

mkdir -p "$ANIMATIONS_DIR"

# Check if we have bundled animations
if [ -d "animations" ]; then
    ANIM_COUNT=$(find animations -maxdepth 1 -name '*.kdl' | wc -l)
    info "Found $ANIM_COUNT bundled animation presets"

    # Copy animations, but don't overwrite existing ones
    for anim in animations/*.kdl; do
        filename=$(basename "$anim")
        target="$ANIMATIONS_DIR/$filename"
        if [ ! -f "$target" ]; then
            cp "$anim" "$target"
            info "  + Copied $filename"
        else
            info "  = $filename already exists (skipped)"
        fi
    done
else
    warn "No bundled animations found (are you running from a full clone?)"
    echo "  You'll need to add .kdl animation files manually to:"
    echo "    $ANIMATIONS_DIR"
fi

# Create default config if none exists
CONFIG_FILE="$CONFIG_DIR/config.kdl"
if [ ! -f "$CONFIG_FILE" ]; then
    {
        echo "// niri-animation-rotate configuration"
        echo "// Uncomment and adjust options as needed:"
        echo ""
        echo "// animation-dir \"$ANIMATIONS_DIR\""
        echo "// animation-target \"$CONFIG_DIR/animation.kdl\""
        echo "// cooldown-ms 0"
        echo "// duration-fallback-ms 500"
        echo "// random-order false"
        echo "// mode \"auto\""
    } > "$CONFIG_FILE"
    info "Created default config: $CONFIG_FILE"
else
    info "Config file already exists: $CONFIG_FILE"
fi

info "Animations directory: $ANIMATIONS_DIR"
info "Animations installed: $(find "$ANIMATIONS_DIR" -maxdepth 1 -name '*.kdl' | wc -l)"

# ──────────────────────────────────────────────
# Step 6: Optional systemd service
# ──────────────────────────────────────────────
if $INSTALL_SYSTEMD; then
    header "6/6 — Installing systemd user service"
    mkdir -p "$SERVICE_DIR"

    cat > "$SERVICE_DIR/$SERVICE_NAME" <<SERVICE
[Unit]
Description=Niri Animation Rotate
After=niri-session.service

[Service]
Type=simple
ExecStart=$BIN_DIR/$BIN_NAME
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
SERVICE

    info "Service file created: $SERVICE_DIR/$SERVICE_NAME"

    # Reload, enable and start
    systemctl --user daemon-reload 2>/dev/null || warn "Could not reload systemd daemon"
    systemctl --user enable "$SERVICE_NAME" 2>/dev/null || warn "Could not enable service (not in a systemd user session?)"
    systemctl --user start "$SERVICE_NAME" 2>/dev/null || warn "Could not start service"

    info "Systemd service installed, enabled and started."
else
    header "6/6 — Skipped (use --systemd to install systemd service)"
fi

# ──────────────────────────────────────────────
# Done
# ──────────────────────────────────────────────
header "✅ Installation complete!"

echo -e "  ${GREEN}$BIN_NAME${NC} has been built and installed."
echo ""
echo -e "${CYAN}Next steps:${NC}"
echo ""
echo "  1. Add this line to your Niri config (~/.config/niri/config.kdl):"
echo ""
echo -e "     ${YELLOW}include \"niri-animation-rotate/animation.kdl\"${NC}"
echo ""
echo "     This tells Niri to read the animation file that the daemon writes."
echo ""
echo "  2. Run the daemon:"
echo ""
echo -e "     ${YELLOW}$BIN_NAME${NC}"
if $INSTALL_SYSTEMD; then
    echo ""
    echo "     (The systemd service is already started and will auto-start on login.)"
else
    echo ""
    echo "     Or if you installed the systemd service:"
    echo ""
    echo -e "     ${YELLOW}systemctl --user start $SERVICE_NAME${NC}"
fi
echo ""
echo "  3. (Optional) To cycle animations manually via keybind, add to Niri config:"
echo ""
echo '     binds {'
echo "         Mod+Shift+A { spawn-sh \"echo 'next' | nc -U \$HOME/.config/niri/niri-animation-rotate/control.sock\"; }"
echo "         Mod+Shift+D { spawn-sh \"echo 'prev' | nc -U \$HOME/.config/niri/niri-animation-rotate/control.sock\"; }"
echo '     }'
echo ""
echo -e "  4. Control the daemon from the terminal with ${YELLOW}nrctl${NC}:"
echo ""
echo -e "     ${YELLOW}nrctl next${NC}            # rotate forward (shows result)"
echo -e "     ${YELLOW}nrctl current${NC}         # show current animation"
echo -e "     ${YELLOW}nrctl list${NC}            # list all animations"
echo -e "     ${YELLOW}nrctl select <name>${NC}   # pick a specific animation"
echo -e "     ${YELLOW}nrctl status${NC}          # show daemon status"
echo -e "     ${YELLOW}nrctl --help${NC}          # full command reference"
echo ""
echo -e "  ${CYAN}Tip:${NC} Use ${YELLOW}--random-order${NC} to shuffle animation order on startup."
echo "       Use ${YELLOW}--cooldown-ms 1000${NC} to add a buffer between rotations."
echo ""
echo -e "${GREEN}Enjoy your animations! 🎬${NC}"
