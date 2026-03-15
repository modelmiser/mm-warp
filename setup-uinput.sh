#!/bin/bash
# Setup script for mm-warp uinput access (no sudo required after setup)

set -e

echo "=== mm-warp uinput Setup ==="
echo ""
echo "This script grants your user access to /dev/uinput"
echo "so you can run mm-warp server without sudo."
echo ""

# Check if already in uinput group
if groups | grep -q uinput; then
    echo "✅ You're already in the uinput group"
else
    echo "➜ Creating uinput group and adding you..."
    sudo groupadd -f uinput
    sudo usermod -a -G uinput $USER
    echo "✅ Added $USER to uinput group"
fi

# Create udev rule
echo ""
echo "➜ Creating udev rule for /dev/uinput..."
echo 'KERNEL=="uinput", GROUP="uinput", MODE="0660"' | sudo tee /etc/udev/rules.d/99-uinput.rules > /dev/null
echo "✅ Created /etc/udev/rules.d/99-uinput.rules"

# Load uinput kernel module
echo ""
echo "➜ Loading uinput kernel module..."
sudo modprobe uinput
echo "✅ uinput module loaded"

# Make it load on boot
echo ""
echo "➜ Configuring uinput to load on boot..."
echo 'uinput' | sudo tee /etc/modules-load.d/uinput.conf > /dev/null
echo "✅ Created /etc/modules-load.d/uinput.conf"

# Reload udev rules
echo ""
echo "➜ Reloading udev rules..."
sudo udevadm control --reload-rules
sudo udevadm trigger
echo "✅ udev rules reloaded"

# Check if /dev/uinput exists and has correct permissions
echo ""
if [ -e /dev/uinput ]; then
    ls -l /dev/uinput
    echo "✅ /dev/uinput exists"
else
    echo "⚠️  /dev/uinput not found - you may need to reboot"
fi

echo ""
echo "========================================="
echo "Setup complete!"
echo ""
echo "⚠️  IMPORTANT: You must log out and log back in"
echo "   (or reboot) for group membership to take effect."
echo ""
echo "After logging back in, run:"
echo "  ./target/release/mm-warp-server"
echo ""
echo "No sudo required! 🎉"
echo "========================================="
