#!/usr/bin/env bash
# ทดสอบเส้นทางหลักในหน้าต่างแอปจริง: WKWebView จริง, IPC จริง, คลัง SQLite จริง, ไฟล์รูปจริงบนดิสก์
# (แนวทางเดียวกับ PuriCat: debug build ฉีด e2e/smoke.js เข้าหน้าต่างหลังโหลดเสร็จ — ดู src-tauri/src/lib.rs `mod e2e`)
#
#   e2e/run.sh              รันเทสต์
#   SHOTS=1 e2e/run.sh      เก็บภาพหน้าต่างทุกขั้นไว้ที่ screenshots/e2e/ (macOS; terminal ต้องได้สิทธิ์ Screen Recording)
#   APP=<binary> e2e/run.sh ทดสอบ debug build ที่ bundle แล้ว (`pnpm tauri build --debug`) แทน `pnpm tauri dev`
#   KEEP=1 e2e/run.sh       ไม่ลบโฟลเดอร์ทำงานหลังจบ (พิมพ์ตำแหน่งไว้)
#
# ใช้คลังและโฟลเดอร์รูปที่สร้างใหม่ในโฟลเดอร์ชั่วคราวทุกครั้ง ไม่แตะคลังจริงหรือรูปจริงของผู้ใช้
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
WORK=$(cd "$(mktemp -d)" && pwd -P)
REPORT="$WORK/report.json"
cd "$ROOT"

cleanup() {
  if [ -n "${APP_PID:-}" ]; then
    kill "$APP_PID" 2>/dev/null || true
  else
    pkill -f "$ROOT/target/debug/puri-photo-search" 2>/dev/null || true
    lsof -ti tcp:1420 2>/dev/null | xargs kill 2>/dev/null || true
  fi
  if [ "${KEEP:-0}" = 1 ]; then echo "  · โฟลเดอร์ทำงาน: $WORK"; else rm -rf "$WORK"; fi
}
trap cleanup EXIT

echo "› สร้างโฟลเดอร์รูปทดสอบ"
PHOTOS=$(cargo run -q -p puri-core --example make_fixtures -- "$WORK/photos")
mkdir -p "$WORK/export"
# สถานะของต้นฉบับก่อนเริ่ม: รายชื่อไฟล์ + SHA-256 + เวลาแก้ไข
snapshot() { (cd "$PHOTOS" && find . -type f -print0 | sort -z | xargs -0 stat -f '%N %m %z' && find . -type f -print0 | sort -z | xargs -0 shasum -a 256); }
snapshot >"$WORK/before.txt"

sed -e "s|__PHOTOS__|$PHOTOS|g" -e "s|__EXPORT__|$WORK/export|g" "${SMOKE:-$ROOT/e2e/smoke.js}" >"$WORK/smoke.js"
if command -v node >/dev/null 2>&1; then
  node --check "$WORK/smoke.js" || { echo "✗ e2e/smoke.js มี syntax error"; exit 1; }
fi

if [ -z "${APP:-}" ] && lsof -ti tcp:1420 >/dev/null 2>&1; then
  echo "✗ port 1420 ถูกใช้อยู่ ปิด dev server ตัวอื่นก่อน" >&2
  exit 1
fi

# native folder picker ขับด้วยสคริปต์ไม่ได้: debug build ตอบคำขอ pick_folder ตามลำดับนี้แทน
# (1) โฟลเดอร์ที่จะเพิ่ม  (2) ปลายทางส่งออก  (3) ปลายทางส่งออกรอบสอง (ที่เดิม เพื่อทดสอบชื่อชน)
export PURI_E2E_PICK="$PHOTOS
$WORK/export
$WORK/export"
export PURI_DATA_DIR="$WORK/library" PURI_DISABLE_OPEN=1 PURI_E2E_SCRIPT="$WORK/smoke.js" PURI_E2E_REPORT="$REPORT"
if [ -n "${APP:-}" ]; then
  echo "› เปิด $APP"
  "$APP" >"$WORK/app.log" 2>&1 &
  APP_PID=$!
else
  echo "› เปิดแอป (pnpm tauri dev)"
  pnpm tauri dev >"$WORK/app.log" 2>&1 &
fi

window_id() {
  swift -e 'import CoreGraphics
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as! [[String: Any]]
var best = (id: 0, area: 0.0)
for w in list where ["puri-photo-search", "puri photo search"].contains((w[kCGWindowOwnerName as String] as? String)?.lowercased() ?? "") {
  let b = w[kCGWindowBounds as String] as? [String: Double] ?? [:]
  let area = (b["Width"] ?? 0) * (b["Height"] ?? 0)
  if area > best.area { best = (w[kCGWindowNumber as String] as! Int, area) }
}
if best.id != 0 { print(best.id) }' 2>/dev/null
}

[ "${SHOTS:-0}" = 1 ] && rm -f "${ROOT:?}"/screenshots/e2e/*.png
last=""
shot=0
window=""
for _ in $(seq 1 2400); do
  sleep 0.25
  [ -f "$REPORT" ] || continue
  stage=$(sed -n 's/^ "stage": "\(.*\)",\{0,1\}$/\1/p' "$REPORT" | head -1)
  [ "$stage" = "$last" ] && continue
  last=$stage
  echo "  · $stage"
  if [ "${SHOTS:-0}" = 1 ] && [ "$stage" != done ] && [ "$stage" != FAILED ]; then
    mkdir -p "$ROOT/screenshots/e2e"
    sleep 0.5
    [ -n "$window" ] || window=$(window_id || true)
    [ -n "$window" ] && screencapture -x -o -l "$window" "$ROOT/screenshots/e2e/$(printf '%02d' $shot)-$stage.png" 2>/dev/null || true
    shot=$((shot + 1))
  fi
  if [ "$stage" = done ] || [ "$stage" = FAILED ]; then break; fi
done

mkdir -p "$ROOT/target"
cp "$REPORT" "$ROOT/target/e2e-report.json" 2>/dev/null || true
if [ "$last" != done ]; then
  echo "✗ e2e ไม่ผ่าน (ขั้นสุดท้าย: ${last:-ยังไม่เริ่ม}) — ดู target/e2e-report.json"
  tail -5 "$WORK/app.log" 2>/dev/null || true
  [ -f "$REPORT" ] && sed -n '/"error"/,/^ *[}\]]/p' "$REPORT" | head -12
  exit 1
fi

# ── ตรวจผลบนดิสก์ ไม่เชื่อแค่สิ่งที่ UI แสดง
fail() { echo "✗ $1"; exit 1; }
snapshot >"$WORK/after.txt"
diff -q "$WORK/before.txt" "$WORK/after.txt" >/dev/null || { diff "$WORK/before.txt" "$WORK/after.txt" | head; fail "ไฟล์ต้นฉบับเปลี่ยน"; }
echo "  · ต้นฉบับทุกไฟล์เหมือนเดิม (รายชื่อ, SHA-256, เวลาแก้ไข)"

exported=$(find "$WORK/export" -type f ! -name '.*' | wc -l | tr -d ' ')
[ "$exported" -ge 1 ] || fail "ไม่มีไฟล์ถูกส่งออก"
renamed=0
for f in "$WORK"/export/*; do
  name=$(basename "$f")
  # ชื่อที่ถูกเติมเลขท้ายเพราะชน เช่น "IMG_0248 (2).jpg" มาจากต้นฉบับชื่อ "IMG_0248.jpg"
  original=$(printf '%s' "$name" | sed -E 's/ \([0-9]+\)(\.[^.]+)$/\1/')
  [ "$original" = "$name" ] || renamed=$((renamed + 1))
  # สำเนาต้องตรงทุก byte กับต้นฉบับไฟล์ใดไฟล์หนึ่งที่ชื่อนี้ (เก็บผลไว้ก่อน grep: pipefail + grep -q ทำให้ท่อแตกได้)
  sums=$(find "$PHOTOS" -type f -name "$original" -exec shasum -a 256 {} +)
  [ -n "$sums" ] || fail "ไฟล์ส่งออก $name ไม่มีต้นฉบับชื่อ $original"
  sum=$(shasum -a 256 "$f" | cut -d' ' -f1)
  case "$sums" in *"$sum"*) ;; *) fail "ไฟล์ส่งออก $name ไม่ตรงกับต้นฉบับ" ;; esac
done
[ "$renamed" -ge 1 ] || fail "ไม่มีไฟล์ที่ถูกเติมเลขท้าย ทั้งที่ส่งออกชื่อซ้ำไปที่เดิม"
[ -z "$(find "$WORK/export" -name '.puri-export-*')" ] || fail "เหลือไฟล์ชั่วคราวในปลายทาง"
echo "  · ส่งออก $exported ไฟล์ ตรงกับต้นฉบับทุก byte ($renamed ไฟล์ถูกเติมเลขท้ายแทนการเขียนทับ) ไม่เหลือไฟล์ชั่วคราว"

db="$WORK/library/data/library/library.sqlite"
q() { sqlite3 "file:$db?mode=ro" "$1"; }
[ "$(q "SELECT COUNT(*) FROM events")" -ge 1 ] || fail "คลังไม่มีกิจกรรม"
[ "$(q "SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL")" -ge 1 ] || fail "คลังไม่มีคำยืนยันใบหน้า"
echo "  · คลัง: $(q "SELECT COUNT(*) FROM events") กิจกรรม, $(q "SELECT COUNT(*) FROM photos") รูป, $(q "SELECT COUNT(*) FROM persons") บุคคล, $(q "SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL") คำยืนยัน"
echo "✓ e2e ผ่าน"
