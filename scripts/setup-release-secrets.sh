#!/bin/bash
# ตั้งค่าให้ GitHub Actions เซ็นและ notarize ตัวติดตั้งเองได้ — รันครั้งเดียว **โดยเจ้าของใบรับรอง** บน Mac ที่มี Developer ID ใน keychain
#
#   scripts/setup-release-secrets.sh [owner/repo]
#
# สคริปต์นี้ส่งค่าต่อไปนี้ขึ้นเป็น secret ของ repo ผ่าน `gh secret set` (GitHub เข้ารหัสไว้ อ่านกลับไม่ได้):
#   APPLE_CERTIFICATE            ใบรับรอง + กุญแจส่วนตัวจาก login keychain (ไฟล์ .p12 แบบ base64)
#   APPLE_CERTIFICATE_PASSWORD   รหัสของไฟล์ .p12 (สุ่มใหม่ทุกครั้งที่รัน)
#   APPLE_SIGNING_IDENTITY       ชื่อใบรับรอง "Developer ID Application: …"
#   APPLE_TEAM_ID                Team ID
#   APPLE_ID                     อีเมล Apple ID (ถามตอนรัน)
#   APPLE_PASSWORD               app-specific password ของ Apple ID (ถามตอนรัน ไม่แสดงบนจอ)
#   TAURI_SIGNING_PRIVATE_KEY    กุญแจเซ็นไฟล์อัปเดตในแอป จาก ~/.config/puriphotosearch/updater.key (หรือที่ PURI_UPDATER_KEY ชี้)
#                                คนละเรื่องกับใบรับรองของ Apple: แอปที่ติดตั้งแล้วรับเฉพาะรุ่นใหม่ที่เซ็นด้วยกุญแจนี้
#                                **ไฟล์นี้มีที่เดียว ทำหายแล้วแอปที่แจกไปจะอัปเดตเองไม่ได้อีก** (กุญแจสาธารณะคู่กันอยู่ใน src-tauri/tauri.conf.json)
#                                ตั้งเฉพาะตัวนี้:  gh secret set TAURI_SIGNING_PRIVATE_KEY --repo <owner/repo> < ~/.config/puriphotosearch/updater.key
#
# ไม่มีค่าใดถูกพิมพ์ออกจอหรือเขียนลงไฟล์ถาวร: ไฟล์ .p12 ชั่วคราวถูกลบเมื่อจบ
# macOS จะถามสิทธิ์เข้าถึงกุญแจใน keychain หนึ่งครั้ง
# หมายเหตุ: `security export` ส่งออกทุก identity ใน login keychain (เช่น Apple Development ด้วย) ถ้าต้องการเฉพาะ Developer ID
# ให้ส่งออกเป็น .p12 จากแอป Keychain Access เอง แล้วรัน:  P12=/path/to/cert.p12 scripts/setup-release-secrets.sh
set -euo pipefail

REPO=${1:-purivaro/PuriPhotoSearch}
command -v gh >/dev/null || { echo "ต้องติดตั้ง GitHub CLI (gh) ก่อน"; exit 1; }

IDENTITY=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)
[ -n "$IDENTITY" ] || { echo "ไม่พบใบรับรอง Developer ID Application ใน keychain"; exit 1; }
TEAM=$(printf '%s' "$IDENTITY" | sed -n 's/.*(\([A-Z0-9]*\))$/\1/p')
echo "repo:     $REPO"
echo "identity: $IDENTITY"
read -r -p "ส่งใบรับรองนี้ขึ้นเป็น secret ของ repo ข้างบน? [y/N] " ok
[ "$ok" = "y" ] || exit 0

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if [ -n "${P12:-}" ]; then
  cp "$P12" "$tmp/cert.p12"
  read -r -s -p "รหัสของไฟล์ .p12: " P12_PASSWORD; echo
else
  P12_PASSWORD=$(uuidgen)
  security export -k "$HOME/Library/Keychains/login.keychain-db" -t identities -f pkcs12 -P "$P12_PASSWORD" -o "$tmp/cert.p12"
fi

base64 <"$tmp/cert.p12" | gh secret set APPLE_CERTIFICATE --repo "$REPO"
printf '%s' "$P12_PASSWORD" | gh secret set APPLE_CERTIFICATE_PASSWORD --repo "$REPO"
printf '%s' "$IDENTITY" | gh secret set APPLE_SIGNING_IDENTITY --repo "$REPO"
printf '%s' "$TEAM" | gh secret set APPLE_TEAM_ID --repo "$REPO"

UPDATER_KEY=${PURI_UPDATER_KEY:-$HOME/.config/puriphotosearch/updater.key}
if [ -f "$UPDATER_KEY" ]; then
  gh secret set TAURI_SIGNING_PRIVATE_KEY --repo "$REPO" <"$UPDATER_KEY"
else
  echo "! ไม่พบกุญแจของตัวอัปเดตที่ $UPDATER_KEY — ข้ามไป (workflow จะออกรุ่นจาก tag ไม่ได้จนกว่าจะตั้ง TAURI_SIGNING_PRIVATE_KEY)"
fi

read -r -p "Apple ID (อีเมล): " APPLE_ID
printf '%s' "$APPLE_ID" | gh secret set APPLE_ID --repo "$REPO"
echo "app-specific password สร้างได้ที่ https://account.apple.com → Sign-In and Security → App-Specific Passwords"
read -r -s -p "App-specific password: " APPLE_PASSWORD; echo
printf '%s' "$APPLE_PASSWORD" | gh secret set APPLE_PASSWORD --repo "$REPO"
unset APPLE_PASSWORD P12_PASSWORD

echo "เสร็จแล้ว: secret ที่ตั้งไว้ใน $REPO"
gh secret list --repo "$REPO"
echo "ทดสอบได้ด้วย: gh workflow run Release --repo $REPO   (build + เซ็น + notarize โดยไม่เผยแพร่)"
