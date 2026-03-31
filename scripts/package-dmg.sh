#!/bin/bash
# Package Peekabrowser.app + Install.command into a custom DMG
# Usage: ./scripts/package-dmg.sh [version] [arch]
# Example: ./scripts/package-dmg.sh 1.4.0          (default: aarch64)
#          ./scripts/package-dmg.sh 1.4.0 x86_64    (Intel)
#          ./scripts/package-dmg.sh 1.4.0 aarch64   (Apple Silicon)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
VERSION="${1:-$(grep '"version"' "$PROJECT_DIR/package.json" | head -1 | sed 's/.*"version": *"\([^"]*\)".*/\1/')}"
ARCH="${2:-aarch64}"

# Set paths based on architecture
if [ "$ARCH" = "x86_64" ]; then
    APP_PATH="$PROJECT_DIR/src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Peekabrowser.app"
    DMG_SUFFIX="x86_64"
else
    APP_PATH="$PROJECT_DIR/src-tauri/target/release/bundle/macos/Peekabrowser.app"
    DMG_SUFFIX="aarch64"
fi

DMG_OUTPUT="$PROJECT_DIR/src-tauri/target/release/bundle/dmg/Peekabrowser_${VERSION}_${DMG_SUFFIX}.dmg"
INSTALL_SCRIPT="$SCRIPT_DIR/Install.command"
TEMP_DIR=$(mktemp -d)
DMG_STAGE="$TEMP_DIR/dmg-stage"

echo "📦 Packaging Peekabrowser v${VERSION} (${ARCH})..."

# Verify app exists
if [ ! -d "$APP_PATH" ]; then
    echo "❌ App not found at: $APP_PATH"
    if [ "$ARCH" = "x86_64" ]; then
        echo "   Run 'cargo tauri build --target x86_64-apple-darwin' first."
    else
        echo "   Run 'cargo tauri build' first."
    fi
    exit 1
fi

# Create staging directory
mkdir -p "$DMG_STAGE"

# Copy app and install script
echo "   Copying Peekabrowser.app..."
cp -R "$APP_PATH" "$DMG_STAGE/"
echo "   Adding Install.command..."
cp "$INSTALL_SCRIPT" "$DMG_STAGE/"
chmod +x "$DMG_STAGE/Install.command"

# Add Applications symlink for drag-and-drop install
ln -s /Applications "$DMG_STAGE/Applications"

# Remove old DMG if exists
rm -f "$DMG_OUTPUT"

# Create DMG
echo "   Creating DMG..."
mkdir -p "$(dirname "$DMG_OUTPUT")"
hdiutil create \
    -volname "Peekabrowser ${VERSION}" \
    -srcfolder "$DMG_STAGE" \
    -ov \
    -format UDZO \
    "$DMG_OUTPUT"

# Cleanup
rm -rf "$TEMP_DIR"

echo ""
echo "✅ DMG created: $DMG_OUTPUT"
echo "   Contents: Peekabrowser.app (${ARCH}), Install.command, Applications (symlink)"
