#!/bin/bash
# สร้าง manifest ของตัวอัปเดตในแอป (latest.json) แล้วพิมพ์ออก stdout
#
#   scripts/update-manifest.sh <version> <base-url> <dir> [notes-file]
#
# <dir>        โฟลเดอร์ที่มีไฟล์อัปเดตในชื่อคงที่ของ release พร้อมลายเซ็น (.sig) ที่ `tauri build` สร้าง:
#              PuriPhotoSearch-mac.app.tar.gz (universal: ใช้ทั้ง Apple Silicon และ Intel) และ PuriPhotoSearch-windows-setup.exe
#              ระบบที่ไม่มี .sig จะไม่อยู่ใน manifest (แอปบนระบบนั้นจะไม่เห็นรุ่นใหม่)
# <base-url>   ที่ที่แอปจะดาวน์โหลดไฟล์เหล่านั้น เช่น https://github.com/<owner>/<repo>/releases/download/v<version>
# [notes-file] ข้อความ "มีอะไรใหม่" ที่แอปแสดงก่อนอัปเดต (ข้อความธรรมดา บรรทัดละเรื่อง) ไม่มีไฟล์ = ไม่แสดง
#
# ใช้โดย workflow Release และ e2e/update.sh; ต้องมี jq
set -euo pipefail
[ $# -ge 3 ] || { sed -n '2,12p' "$0" >&2; exit 2; }
version=$1 base=${2%/} dir=$3 notes_file=${4:-}

notes=""
if [ -n "$notes_file" ] && [ -f "$notes_file" ]; then notes=$(cat "$notes_file"); fi

platforms='{}'
add() { # <platform ใน manifest> <ชื่อไฟล์>
  [ -f "$dir/$2.sig" ] && [ -f "$dir/$2" ] || return 0
  platforms=$(jq --arg key "$1" --arg url "$base/$2" --rawfile sig "$dir/$2.sig" \
    '.[$key] = { signature: ($sig | gsub("\\s"; "")), url: $url }' <<<"$platforms")
}
add darwin-aarch64 PuriPhotoSearch-mac.app.tar.gz
add darwin-x86_64 PuriPhotoSearch-mac.app.tar.gz
add windows-x86_64 PuriPhotoSearch-windows-setup.exe
[ "$platforms" != '{}' ] || { echo "ไม่พบไฟล์อัปเดตที่มีลายเซ็นใน $dir" >&2; exit 1; }

jq -n --arg version "$version" --arg notes "$notes" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson platforms "$platforms" \
  '{ version: $version, notes: $notes, pub_date: $date, platforms: $platforms }'
