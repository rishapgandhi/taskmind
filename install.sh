#!/bin/bash
# TaskMind Installer — handles everything automatically.
# Usage: git clone ... && cd taskmind && bash install.sh
set -e

APP="TaskMind"
INSTALL_DIR="$HOME/.local/share/taskmind"
CONFIG_DIR="$HOME/.config/taskmind"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXTENSION_UUID="window-calls@domandoman.xyz"
EXTENSION_URL="https://extensions.gnome.org/extension-data/window-callsdomandoman.xyz.v11.shell-extension.zip"

echo "🧠 Installing $APP..."
echo ""

# --- 1. System dependencies ---
echo "[1/7] Installing system dependencies..."
sudo apt-get update -qq
sudo apt-get install -y -qq \
    python3 python3-pip python3-venv \
    xdotool xprintidle libnotify-bin \
    wmctrl wget unzip 2>/dev/null || true

# --- 2. Detect Wayland and install GNOME extension if needed ---
echo "[2/7] Detecting display server..."
SESSION_TYPE="${XDG_SESSION_TYPE:-x11}"
NEEDS_RELOGIN=false

if [ "$SESSION_TYPE" = "wayland" ]; then
    echo "  Detected: Wayland (GNOME)"
    echo "  Installing Window Calls extension for Wayland window tracking..."
    
    EXT_DIR="$HOME/.local/share/gnome-shell/extensions/$EXTENSION_UUID"
    mkdir -p "$EXT_DIR"
    
    wget -q "$EXTENSION_URL" -O /tmp/window-calls.zip 2>/dev/null || \
        curl -sL "$EXTENSION_URL" -o /tmp/window-calls.zip 2>/dev/null || true
    
    if [ -f /tmp/window-calls.zip ]; then
        unzip -o /tmp/window-calls.zip -d "$EXT_DIR" >/dev/null 2>&1
        gnome-extensions enable "$EXTENSION_UUID" 2>/dev/null || true
        NEEDS_RELOGIN=true
        echo "  ✓ Extension installed"
    else
        echo "  ⚠ Could not download extension (no internet?). Window tracking may not work."
    fi
    rm -f /tmp/window-calls.zip
else
    echo "  Detected: X11 (xdotool will be used)"
fi

# --- 3. Create directories ---
echo "[3/7] Creating directories..."
mkdir -p "$INSTALL_DIR/logs"
mkdir -p "$INSTALL_DIR/recordings"
mkdir -p "$CONFIG_DIR"

# --- 4. Python virtual environment ---
echo "[4/7] Setting up Python environment..."
python3 -m venv "$INSTALL_DIR/venv"
source "$INSTALL_DIR/venv/bin/activate"
pip install --quiet --upgrade pip
pip install --quiet -e "$SCRIPT_DIR"
pip install --quiet fastapi uvicorn

# Install git hook so 'git pull' auto-restarts daemon
if [ -d "$SCRIPT_DIR/.git/hooks" ]; then
    cat > "$SCRIPT_DIR/.git/hooks/post-merge" << 'HOOK'
#!/bin/bash
if command -v taskmind &>/dev/null; then
    taskmind stop 2>/dev/null
    taskmind start 2>/dev/null
    echo "✅ TaskMind daemon restarted with latest code."
fi
HOOK
    chmod +x "$SCRIPT_DIR/.git/hooks/post-merge"
fi

# --- 5. Config files ---
echo "[5/7] Setting up configuration..."
if [ ! -f "$CONFIG_DIR/config.yaml" ]; then
    cp "$SCRIPT_DIR/configs/config.default.yaml" "$CONFIG_DIR/config.yaml"
fi
if [ ! -f "$CONFIG_DIR/projects.yaml" ]; then
    cp "$SCRIPT_DIR/configs/projects.example.yaml" "$CONFIG_DIR/projects.yaml"
fi

# --- 6. Systemd service (auto-start on login) ---
echo "[6/7] Installing systemd service..."
mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/taskmind.service" << EOF
[Unit]
Description=TaskMind Activity Tracker
After=graphical-session.target

[Service]
Type=simple
ExecStart=$INSTALL_DIR/venv/bin/python -m taskmind.daemon
Restart=on-failure
RestartSec=5
Environment=DISPLAY=:0
Environment=XAUTHORITY=$HOME/.Xauthority
Environment=XDG_SESSION_TYPE=$SESSION_TYPE
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable taskmind 2>/dev/null || true

# --- 7. Add CLI to PATH ---
echo "[7/7] Setting up CLI..."
mkdir -p "$HOME/.local/bin"
ln -sf "$INSTALL_DIR/venv/bin/taskmind" "$HOME/.local/bin/taskmind"

# Ensure ~/.local/bin is in PATH
if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
    for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
        if [ -f "$rc" ] && ! grep -q 'local/bin' "$rc"; then
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$rc"
        fi
    done
    export PATH="$HOME/.local/bin:$PATH"
fi

# --- Done ---
echo ""
echo "═══════════════════════════════════════════════════════"
echo "  ✅ $APP installed successfully!"
echo "═══════════════════════════════════════════════════════"
echo ""
echo "  Start tracking:    taskmind start"
echo "  Check status:      taskmind status"
echo "  Today's recap:     taskmind today"
echo "  Auto timesheet:    taskmind timesheet"
echo "  Export CSV:        taskmind timesheet --export csv"
echo "  Web dashboard:     taskmind dashboard"
echo ""
echo "  Config:   $CONFIG_DIR/config.yaml"
echo "  Projects: $CONFIG_DIR/projects.yaml"
echo ""

if [ "$NEEDS_RELOGIN" = true ]; then
    echo "═══════════════════════════════════════════════════════"
    echo "  ⚠️  ACTION REQUIRED (Wayland users only):"
    echo "  Log out and log back in once to activate the"
    echo "  window tracking extension. Then run:"
    echo ""
    echo "      taskmind setup"
    echo "      taskmind start"
    echo "═══════════════════════════════════════════════════════"
else
    # Run interactive project setup
    echo ""
    echo "═══════════════════════════════════════════════════════"
    echo ""
    "$INSTALL_DIR/venv/bin/taskmind" setup
    echo ""
    echo "  Starting daemon now..."
    systemctl --user start taskmind 2>/dev/null || \
        ($INSTALL_DIR/venv/bin/python -m taskmind.daemon &)
    echo "  ● TaskMind is now tracking."
fi
