#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Firelink"
VERSION="${VERSION:-}"
ARCH="${ARCH:-arm64}"
SIGNING_IDENTITY="${CODE_SIGN_IDENTITY:-${SIGNING_IDENTITY:-}}"
APP_DIR="$ROOT_DIR/build/$APP_NAME.app"
DIST_DIR="$ROOT_DIR/dist"
DMG_STAGING_DIR="$ROOT_DIR/build/dmg"

if [[ ! -d "$APP_DIR" ]]; then
  echo "Missing app bundle: $APP_DIR" >&2
  echo "Run Scripts/create_app_bundle.sh first." >&2
  exit 1
fi

if [[ -z "$VERSION" ]]; then
  VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_DIR/Contents/Info.plist")"
fi

DMG_PATH="$DIST_DIR/$APP_NAME-$VERSION-mac-$ARCH.dmg"

rm -rf "$DMG_STAGING_DIR"
mkdir -p "$DMG_STAGING_DIR" "$DIST_DIR"
cp -R "$APP_DIR" "$DMG_STAGING_DIR/"
ln -s /Applications "$DMG_STAGING_DIR/Applications"

rm -f "$DMG_PATH"
hdiutil create \
  -volname "$APP_NAME $VERSION" \
  -srcfolder "$DMG_STAGING_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

if [[ -n "$SIGNING_IDENTITY" ]]; then
  codesign --force --timestamp --sign "$SIGNING_IDENTITY" "$DMG_PATH"
fi

echo "Created $DMG_PATH"
