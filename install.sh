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
GITHUB_API="https://api.github.com/repos/pnbarbeito/niri-animation-rotate/releases/latest"
RELEASE_ARCHIVE="niri-animation-rotate-linux-x86_64.tar.gz"

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

download_release() {
    header "$STEP/$TOTAL — Downloading precompiled binary"

    local download_url
    info "Fetching latest release info..."
    download_url=$(curl -fsSL "$GITHUB_API" | grep "browser_download_url.*$RELEASE_ARCHIVE" | cut -d '"' -f 4 | head -1)

    if [ -z "$download_url" ]; then
        error "Could not find release download URL. Try --source to build from source instead."
        exit 1
    fi

    info "Downloading: $download_url"
    local tmp_archive="/tmp/$RELEASE_ARCHIVE"
    curl -fsSL "$download_url" -o "$tmp_archive" || {
        error "Download failed."
        exit 1
    }

    info "Extracting..."
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR" "$tmp_archive"' EXIT
    tar xzf "$tmp_archive" -C "$TMP_DIR" || {
        error "Extraction failed."
        exit 1
    }
    REPO_DIR="$TMP_DIR/niri-animation-rotate"

    if [ ! -f "$REPO_DIR/niri-animation-rotate" ]; then
        error "Precompiled binary not found in release archive."
        exit 1
    fi

    info "Precompiled binary downloaded and verified."
    cd "$REPO_DIR"
}

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Options:
  --release    Download precompiled binary from GitHub Releases (fast, no Rust needed)
  --source     Build from source code (requires Rust/Cargo)
  --systemd    Install and enable a systemd user service
  --help       Show this help message and exit

If neither --release nor --source is given, you will be prompted to choose.

Environment:
  CARGO_HOME  Path to Cargo installation (default: ~/.cargo)

The installer will:
  1. Download or build niri-animation-rotate
  2. Install the binary and nrctl to ~/.local/bin/
  3. Create config directories under ~/.config/niri/niri-animation-rotate/
  4. Copy bundled animations (49 presets) to the config directory
  5. Optionally set up a systemd user service (with --systemd)
  6. Print next steps for Niri configuration
EOF
    exit 0
}

# ──────────────────────────────────────────────
# Parse arguments
# ──────────────────────────────────────────────
INSTALL_SYSTEMD=false
MODE=""  # release or source
for arg in "$@"; do
    case "$arg" in
        --help) usage ;;
        --release) MODE="release" ;;
        --source)  MODE="source" ;;
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
# Prompt for install method if not specified
# ──────────────────────────────────────────────
if [ -z "$MODE" ]; then
    echo ""
    echo "How would you like to install niri-animation-rotate?"
    echo ""
    echo "  [1] Download precompiled binary (fast, no dependencies)"
    echo "  [2] Build from source (requires Rust/Cargo)"
    echo ""
    read -r -p "Choice [1/2]: " choice
    case "$choice" in
        1) MODE="release" ;;
        2) MODE="source" ;;
        *) error "Invalid choice. Please run again and select 1 or 2."; exit 1 ;;
    esac
fi

# ──────────────────────────────────────────────
# Step 1: Get the binary (download or build)
# ──────────────────────────────────────────────
STEP=1
TOTAL=5  # will adjust for source builds

if [ "$MODE" = "release" ]; then
    download_release
else
    TOTAL=6
    # ── Source build path ─────────────────────
    header "$STEP/$TOTAL — Checking prerequisites"

    # Check for Rust/Cargo
    if command -v cargo &>/dev/null; then
        info "Rust/Cargo found: $(cargo --version | head -1)"
    else
        warn "Rust/Cargo not found."
        echo ""
        echo "  Install the Rust toolchain with:"
        echo "      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        echo ""
        echo "  Then restart your shell and run this installer again."
        echo ""
        exit 1
    fi

    # Get source
    STEP=2
    header "$STEP/$TOTAL — Getting source code"

    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
    IN_REPO=false
    if [ -f "$SCRIPT_DIR/Cargo.toml" ] && grep -q 'name = "niri-animation-rotate"' "$SCRIPT_DIR/Cargo.toml" 2>/dev/null; then
        IN_REPO=true
        REPO_DIR="$SCRIPT_DIR"
        info "Found local repo at $REPO_DIR"
        cd "$REPO_DIR"
    else
        if ! command -v git &>/dev/null; then
            error "Git is required to clone the repository. Install git and try again."
            exit 1
        fi
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

    # Build
    STEP=3
    header "$STEP/$TOTAL — Building binary"
    info "Running cargo build --release (this may take a few minutes)..."
    if cargo build --release; then
        info "Build successful!"
    else
        error "Build failed. Please check the output above for errors."
        exit 1
    fi
fi

# ──────────────────────────────────────────────
# Step N: Install binary (for both modes)
# ──────────────────────────────────────────────
STEP=$((STEP + 1))
header "$STEP/$TOTAL — Installing binary"

# Stop any running instance so the old binary can be safely replaced
if $INSTALL_SYSTEMD; then
    systemctl --user stop "$SERVICE_NAME" 2>/dev/null || true
fi

mkdir -p "$BIN_DIR"
rm -f "$BIN_DIR/$BIN_NAME"     # allow overwriting even if daemon is running

if [ "$MODE" = "release" ]; then
    cp "$REPO_DIR/niri-animation-rotate" "$BIN_DIR/$BIN_NAME"
else
    cp "target/release/$BIN_NAME" "$BIN_DIR/"
fi
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
STEP=$((STEP + 1))
header "$STEP/$TOTAL — Setting up animations"

mkdir -p "$ANIMATIONS_DIR"

# Check if we have bundled animations
if [ -d "animations" ]; then
    ANIM_COUNT=$(find animations -maxdepth 1 -name '*.kdl' | wc -l)
    info "Found $ANIM_COUNT bundled animation presets"

    # Copy animations, but don't overwrite existing ones
    for anim in animations/*.kdl animations/*.kld; do
        [ -f "$anim" ] || continue
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
    STEP=$((STEP + 1))
    header "$STEP/$TOTAL — Installing systemd user service"
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
    header "$TOTAL/$TOTAL — Skipped (use --systemd to install systemd service)"
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
echo -e "       Use ${YELLOW}--cooldown-ms 1000${NC} to add a buffer between rotations."
echo ""
echo -e "${GREEN}Enjoy your animations! 🎬${NC}"
