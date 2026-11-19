#!/usr/bin/env bash
# พิสูจน์ว่าแอปรุ่นที่เผยแพร่แล้ว (release build จริง: เซ็น, hardened runtime, notarize) อัปเดตตัวเองเป็นรุ่นล่าสุดได้ — macOS
#
#   e2e/update-published.sh [tag ของรุ่นเก่า]     ไม่ระบุ = release ก่อนหน้ารุ่นล่าสุด
#   KEEP=1 e2e/update-published.sh               ไม่ลบโฟลเดอร์ทำงานหลังจบ
#
# รันหลังออกรุ่นทุกครั้ง: ติดตั้งแอปของ tag เก่าในโฟลเดอร์ชั่วคราว เปิดด้วย HOME ชั่วคราว (ไม่แตะคลังจริง) รอให้มันพบและดาวน์โหลด
# รุ่นใหม่เองจาก GitHub แล้วกดปุ่ม "อัปเดต" → "อัปเดตและเปิดใหม่" ในหน้าต่างแทนผู้ใช้ผ่าน Accessibility (e2e/ax.swift)
# เพราะ release build ไม่มี hook ของเทสต์ — ต่างจาก e2e/update.sh ซึ่งฝ่ายที่ถามหารุ่นใหม่เป็น debug build
#
# ต้องมี: โปรแกรมที่รันสคริปต์นี้ได้สิทธิ์ Accessibility, gh, jq, swiftc และ release เก่าต้องมีตัวอัปเดต (2.0.0-alpha.4 ขึ้นไป)
# ระหว่างรัน หน้าต่างของแอปทดสอบจะถูกดึงขึ้นมาข้างหน้า (Accessibility เห็นเฉพาะหน้าต่างบน Space ที่กำลังแสดง)
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=purivaro/PuriPhotoSearch
FROM=${1:-$(gh release list --repo "$REPO" --limit 2 --json tagName -q '.[1].tagName')}
[ -n "$FROM" ] || { echo "✗ หา release ก่อนหน้าไม่ได้"; exit 1; }
fail() { echo "✗ $1"; exit 1; }
has() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac; }
# Accessibility เห็นเฉพาะหน้าต่างบน Space ที่กำลังแสดง: ดึงแอปขึ้นมาข้างหน้าก่อนอ่านหรือกดทุกครั้ง
front() { osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $1) to true" >/dev/null 2>&1 || true; sleep 0.4; }
seen() { front "$(pid)"; "$AX" "$(pid)" list 2>/dev/null || true; }
press() { front "$1"; "$AX" "$1" click "$2"; }
NEW=$(curl -fsSL "https://raw.githubusercontent.com/$REPO/updates/latest.json" | jq -r .version)
[ "v$NEW" != "$FROM" ] || fail "manifest ยังเป็นรุ่น $NEW เท่ากับรุ่นเก่าที่จะทดสอบ"
WORK=$(cd "$(mktemp -d)" && pwd -P)
REAL="$HOME/Library/Application Support/com.purivaro.puriphotosearch2/library/library.sqlite"
state() { if [ -f "$REAL" ]; then stat -f '%m %z' "$REAL"; else echo none; fi; }
BEFORE=$(state)
app="$WORK/Applications/Puri Photo Search.app"
bin="$app/Contents/MacOS/puri-photo-search"
version() { /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist"; }
pid() { pgrep -f "$bin" | head -1; }
cleanup() { pkill -f "$WORK/Applications" 2>/dev/null || true; [ "${KEEP:-0}" = 1 ] && echo "  · $WORK" || rm -rf "$WORK"; }
trap cleanup EXIT
AX="$WORK/ax"
swiftc -O "$HERE/ax.swift" -o "$AX" 2>"$WORK/swiftc.log" || { tail -5 "$WORK/swiftc.log"; fail "compile e2e/ax.swift ไม่ผ่าน"; }
"$AX" trusted || fail "โปรแกรมที่รันสคริปต์นี้ยังไม่ได้สิทธิ์ Accessibility (System Settings → Privacy & Security → Accessibility)"

echo "› ติดตั้งแอป $FROM ที่เผยแพร่ (ไฟล์อัปเดตของ release นั้น)"
mkdir -p "$WORK/Applications" "$WORK/home" "$WORK/dl"
gh release download "$FROM" --repo "$REPO" -p PuriPhotoSearch-mac.app.tar.gz -D "$WORK/dl" >/dev/null
tar -xzf "$WORK/dl/PuriPhotoSearch-mac.app.tar.gz" -C "$WORK/Applications"
[ "v$(version)" = "$FROM" ] || fail "แอปที่ติดตั้งไม่ใช่ $FROM"
has "$(spctl -a -vv "$app" 2>&1 || true)" "Notarized Developer ID" || fail "แอปรุ่นเก่าไม่ใช่ตัวที่ notarize"
has "$(codesign -dv "$app" 2>&1 || true)" "(runtime)" || fail "แอปรุ่นเก่าไม่ได้เปิด hardened runtime"

echo "› เปิดแอป $FROM แล้วรอให้พบรุ่น $NEW เอง"
(HOME="$WORK/home" nohup "$bin" >"$WORK/app.log" 2>&1 &)
started=$(date +%s)
clicked=""
for _ in $(seq 1 180); do
  sleep 1
  p=$(pid) || true
  [ -n "${p:-}" ] || continue
  if press "$p" "อัปเดต" >/dev/null 2>&1; then clicked=1; break; fi
done
[ -n "$clicked" ] || { seen | tail -15; fail "ไม่พบปุ่ม “อัปเดต” ของ toast ภายใน 3 นาที"; }
echo "  · แอปพบรุ่นใหม่ ดาวน์โหลดและตรวจลายเซ็นเอง แล้วขึ้น toast (หลังเปิด $(( $(date +%s) - started )) วินาที) — กด “อัปเดต”"
sleep 1.5
dialog=$(seen)
printf '%s\n' "$dialog" | grep -A8 "อัปเดตเป็นรุ่น" | sed 's/^/      /' || true
has "$dialog" "อัปเดตเป็นรุ่น $NEW" || fail "กล่องยืนยันไม่ได้บอกรุ่น $NEW"
old_pid=$(pid)
press "$old_pid" "อัปเดตและเปิดใหม่" >/dev/null || fail "กดปุ่ม “อัปเดตและเปิดใหม่” ไม่ได้"
echo "  · กด “อัปเดตและเปิดใหม่”"
for _ in $(seq 1 60); do
  sleep 1
  p=$(pid) || true
  if [ "$(version)" = "$NEW" ] && [ -n "${p:-}" ] && [ "$p" != "$old_pid" ]; then break; fi
done
[ "$(version)" = "$NEW" ] || fail "แอปในตำแหน่งเดิมยังไม่ใช่รุ่น $NEW (ได้ $(version))"
p=$(pid) || true
[ -n "${p:-}" ] && [ "$p" != "$old_pid" ] || fail "แอปไม่ได้เปิดขึ้นมาใหม่"
sleep 6
codesign --verify --deep --strict "$app" || fail "ลายเซ็นของแอปหลังอัปเดตไม่สมบูรณ์"
has "$(spctl -a -vv "$app" 2>&1 || true)" "Notarized Developer ID" || fail "Gatekeeper ไม่รับแอปหลังอัปเดต"
[ -z "$(find "$WORK/Applications" -mindepth 1 -maxdepth 1 ! -name "Puri Photo Search.app")" ] || fail "เหลือไฟล์ชั่วคราวข้างแอป"
# WebKit สร้างต้นไม้ accessibility หลังถูกถามครั้งแรก: ถามซ้ำจนเห็นหน้าจอ
shown=""
for _ in $(seq 1 10); do if has "$(seen)" "PURI PHOTO SEARCH"; then shown=1; break; fi; sleep 1.5; done
[ -n "$shown" ] || fail "แอปรุ่นใหม่ไม่ได้แสดงหน้าจอ"
[ "$(state)" = "$BEFORE" ] || fail "คลังจริงเปลี่ยนระหว่างเทสต์"
echo "  · แอปในตำแหน่งเดิมเป็นรุ่น $NEW (notarize, Gatekeeper รับ), เปิดขึ้นมาใหม่เองและแสดงหน้าจอ, ไม่เหลือไฟล์ชั่วคราว, คลังจริงไม่เปลี่ยน"
echo "✓ แอปที่เผยแพร่อัปเดตตัวเอง: $FROM → v$NEW"
