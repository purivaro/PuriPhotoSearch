#!/bin/bash
# สร้างตัวติดตั้ง macOS ด้วยมือ บน Mac ที่มีใบรับรองเซ็นแอป (แนวทางเดียวกับ PuriCat):
# แอป universal (Apple Silicon + Intel) เซ็นด้วย Developer ID จาก keychain,
# DMG ถูก notarize ผ่านโปรไฟล์ keychain ของ notarytool แล้ว staple
#
#   APPLE_SIGNING_IDENTITY="Developer ID Application: …" \
#   PURI_NOTARY_PROFILE=<profile> scripts/release-mac.sh
#
# โปรไฟล์คือชื่อที่สร้างด้วย `xcrun notarytool store-credentials`
# ไม่ตั้ง PURI_NOTARY_PROFILE = เซ็นแต่ไม่ notarize; ไม่ตั้ง APPLE_SIGNING_IDENTITY = ad-hoc เท่านั้น (เครื่องอื่นจะเปิดไม่ได้ตามปกติ)
# สคริปต์นี้ไม่อ่านและไม่เขียนความลับ: codesign และ notarytool หยิบจาก keychain เอง
#
# ไฟล์อัปเดตในแอป: ถ้ามีกุญแจของตัวอัปเดตที่ ~/.config/puriphotosearch/updater.key (หรือที่ PURI_UPDATER_KEY ชี้) จะได้
# PuriPhotoSearch-mac.app.tar.gz + .sig ด้วย (Tauri อ่านกุญแจจากไฟล์นั้นเอง) — manifest ที่แอปใช้ถามหารุ่นใหม่เผยแพร่โดย workflow Release เท่านั้น
#
# ผลลัพธ์: target/release-download/PuriPhotoSearch-mac.dmg และ SHA256SUMS.txt
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
export APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:--}"

bundle=target/universal-apple-darwin/release/bundle
rm -f "$bundle"/dmg/*.dmg "$bundle"/macos/*.tar.gz*
updater='{}'
updater_key="${PURI_UPDATER_KEY:-$HOME/.config/puriphotosearch/updater.key}"
if [ -f "$updater_key" ]; then
  export TAURI_SIGNING_PRIVATE_KEY="$updater_key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
  updater='{"bundle":{"createUpdaterArtifacts":true}}'
else
  echo "! ไม่พบกุญแจของตัวอัปเดต ($updater_key): ได้ตัวติดตั้งอย่างเดียว ไม่มีไฟล์อัปเดตในแอป"
fi
pnpm tauri build --target universal-apple-darwin --config "$updater"
app="$bundle/macos/Puri Photo Search.app"
dmg=$(ls "$bundle"/dmg/*.dmg)
codesign --verify --deep --strict --verbose=2 "$app"
lipo -archs "$app/Contents/MacOS/puri-photo-search"

if [ "$APPLE_SIGNING_IDENTITY" != "-" ]; then
  # Tauri เซ็นตัวแอปแล้ว; DMG ที่ห่อแอปต้องมีลายเซ็นของตัวเอง
  codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$dmg"
  if [ -n "${PURI_NOTARY_PROFILE:-}" ]; then
    xcrun notarytool submit "$dmg" --keychain-profile "$PURI_NOTARY_PROFILE" --wait
    xcrun stapler staple "$dmg"
    xcrun stapler validate "$dmg"
    spctl -a -vv -t install "$dmg"
  else
    echo "! ไม่ได้ตั้ง PURI_NOTARY_PROFILE: เซ็นแล้ว แต่ไม่ได้ notarize"
  fi
else
  echo "! ไม่ได้ตั้ง APPLE_SIGNING_IDENTITY: เซ็นแบบ ad-hoc เท่านั้น"
fi

out=target/release-download
mkdir -p "$out"
rm -f "$out"/PuriPhotoSearch-mac.*
cp "$dmg" "$out/PuriPhotoSearch-mac.dmg"
if [ -f "$app.tar.gz.sig" ]; then
  cp "$app.tar.gz" "$out/PuriPhotoSearch-mac.app.tar.gz"
  cp "$app.tar.gz.sig" "$out/PuriPhotoSearch-mac.app.tar.gz.sig"
fi
(cd "$out" && shasum -a 256 PuriPhotoSearch-mac.dmg > SHA256SUMS.txt && cat SHA256SUMS.txt)
ls -la "$out"
