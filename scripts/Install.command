#!/bin/bash
# Peekabrowser Installer
# 雙擊此檔案即可安裝 / Double-click to install

set -e

APP_NAME="Peekabrowser.app"
INSTALL_DIR="/Applications"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SOURCE_APP="$SCRIPT_DIR/$APP_NAME"

echo ""
echo "=========================================="
echo "  Peekabrowser Installer"
echo "=========================================="
echo ""

# Check if the app exists in the DMG
if [ ! -d "$SOURCE_APP" ]; then
    echo "❌ 找不到 $APP_NAME / Cannot find $APP_NAME"
    echo "   請確認此腳本與 Peekabrowser.app 在同一個資料夾"
    echo "   Make sure this script is in the same folder as Peekabrowser.app"
    echo ""
    read -p "按 Enter 關閉 / Press Enter to close..."
    exit 1
fi

# Kill running instance if any
if pgrep -x "peekabrowser" > /dev/null 2>&1; then
    echo "⏳ 正在關閉執行中的 Peekabrowser..."
    echo "   Closing running Peekabrowser..."
    killall peekabrowser 2>/dev/null || true
    sleep 1
fi

# Copy to Applications
echo "📦 正在安裝到 $INSTALL_DIR..."
echo "   Installing to $INSTALL_DIR..."
cp -R "$SOURCE_APP" "$INSTALL_DIR/"

# Remove quarantine attribute
echo "🔓 移除 macOS 隔離標記..."
echo "   Removing macOS quarantine flag..."
xattr -cr "$INSTALL_DIR/$APP_NAME"

echo ""
echo "✅ 安裝完成！正在啟動 Peekabrowser..."
echo "   Installation complete! Launching Peekabrowser..."
echo ""

# Launch the app
open "$INSTALL_DIR/$APP_NAME"

# Close terminal window after 2 seconds
sleep 2
osascript -e 'tell application "Terminal" to close front window' 2>/dev/null || true
