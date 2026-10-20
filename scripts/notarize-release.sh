#!/bin/bash
# Build both arches signed with Developer ID, notarize + staple each .app, then
# build, sign, notarize and staple the custom DMG and write SHA256SUMS.
# Usage: ./scripts/notarize-release.sh [version]   (SKIP_BUILD=1 to reuse existing builds)
# Needs a notarytool keychain profile (default: peekabrowser-notary), created once with:
#   xcrun notarytool store-credentials peekabrowser-notary --key <AuthKey_XXX.p8> --key-id <KEY_ID> --issuer <ISSUER_ID>
# (App Store Connect API key; no Apple ID password needed)

set -e
export PATH="/usr/bin:$PATH"  # avoid non-Apple codesign shadowing (e.g. anaconda)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
VERSION="${1:-$(grep '"version"' "$PROJECT_DIR/package.json" | head -1 | sed 's/.*"version": *"\([^"]*\)".*/\1/')}"
PROFILE="${NOTARY_PROFILE:-peekabrowser-notary}"
IDENTITY="${SIGN_IDENTITY:-Developer ID Application: Weiling Kang (SYPQNK76T4)}"
OUT_DIR="$PROJECT_DIR/release/v$VERSION"
mkdir -p "$OUT_DIR"

for pair in "aarch64 aarch64-apple-darwin" "x64 x86_64-apple-darwin"; do
    read -r ARCH TRIPLE <<< "$pair"
    if [ -z "$SKIP_BUILD" ]; then
        echo "== $ARCH: building"
        (cd "$PROJECT_DIR" && APPLE_SIGNING_IDENTITY="$IDENTITY" npx tauri build --target "$TRIPLE")
    fi
    BUNDLE_DIR="$PROJECT_DIR/src-tauri/target/$TRIPLE/release/bundle"
    APP="$BUNDLE_DIR/macos/Peekabrowser.app"
    DMG="$BUNDLE_DIR/dmg/Peekabrowser_${VERSION}_${ARCH}.dmg"

    echo "== $ARCH: notarizing app"
    /usr/bin/codesign --verify --deep --strict "$APP"
    ZIP="$(mktemp -d)/Peekabrowser.zip"
    /usr/bin/ditto -c -k --keepParent "$APP" "$ZIP"
    xcrun notarytool submit "$ZIP" --keychain-profile "$PROFILE" --wait
    xcrun stapler staple "$APP"

    echo "== $ARCH: building DMG"
    "$SCRIPT_DIR/package-dmg.sh" "$VERSION" "$ARCH" "$TRIPLE"
    /usr/bin/codesign -f -s "$IDENTITY" --timestamp "$DMG"
    xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
    xcrun stapler staple "$DMG"
    spctl -a -t open --context context:primary-signature -vv "$DMG"
    cp "$DMG" "$OUT_DIR/"
done

(cd "$OUT_DIR" && shasum -a 256 Peekabrowser_*.dmg > SHA256SUMS.txt && cat SHA256SUMS.txt)
echo "✅ Release files in $OUT_DIR"
