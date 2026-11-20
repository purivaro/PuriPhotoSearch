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
  if [ "${KEEP:-0}" = 1 ]; then
    echo "  · โฟลเดอร์ทำงาน: $WORK"
  else
    # Windows: ไฟล์คลังยังถูกแอปถือไว้จนกว่า process จะจบจริง — รอให้จบ และไม่ให้การลบโฟลเดอร์ชั่วคราวทำให้เทสต์ที่ผ่านแล้วล้ม
    if [ -n "${APP_PID:-}" ]; then wait "$APP_PID" 2>/dev/null || true; fi
    rm -rf "$WORK" 2>/dev/null || { sleep 2; rm -rf "$WORK" 2>/dev/null || echo "  · ลบโฟลเดอร์ชั่วคราวไม่หมด: $WORK"; }
  fi
}
trap cleanup EXIT

# Windows (Git Bash): แอปต้องได้ path แบบของ Windows (C:\…) ส่วนคำสั่งใน shell ใช้ path แบบ POSIX ต่อไป
native() { if command -v cygpath >/dev/null 2>&1; then cygpath -w "$1"; else printf '%s' "$1"; fi; }
# path สำหรับแทนลงในสคริปต์ JS ผ่าน sed: backslash หนึ่งตัวต้องกลายเป็นสองตัวในไฟล์ JS
js_path() { native "$1" | sed 's/\\/\\\\\\\\/g'; }
# เครื่องมือของ macOS (BSD) กับ Git Bash (GNU) ใช้ตัวเลือกต่างกัน
if stat -f '%N' . >/dev/null 2>&1; then STAT=(stat -f '%N %m %z'); else STAT=(stat -c '%n %Y %s'); fi
if command -v shasum >/dev/null 2>&1; then SHA=(shasum -a 256); else SHA=(sha256sum); fi

echo "› สร้างโฟลเดอร์รูปทดสอบ"
# make_fixtures สร้างโฟลเดอร์แม่ชื่อ 2569 ไว้ใต้ปลายทางที่ระบุ
PHOTOS="$WORK/photos/2569"
cargo run -q -p puri-core --example make_fixtures -- "$(native "$WORK/photos")" >/dev/null
mkdir -p "$WORK/export"
# สถานะของต้นฉบับก่อนเริ่ม: รายชื่อไฟล์ + SHA-256 + เวลาแก้ไข
snapshot() { (cd "$PHOTOS" && find . -type f -print0 | sort -z | xargs -0 "${STAT[@]}" && find . -type f -print0 | sort -z | xargs -0 "${SHA[@]}"); }
snapshot >"$WORK/before.txt"

sed -e "s|__PHOTOS__|$(js_path "$PHOTOS")|g" -e "s|__EXPORT__|$(js_path "$WORK/export")|g" "${SMOKE:-$ROOT/e2e/smoke.js}" >"$WORK/smoke.js"
if command -v node >/dev/null 2>&1; then
  node --check "$WORK/smoke.js" || { echo "✗ e2e/smoke.js มี syntax error"; exit 1; }
fi

if [ -z "${APP:-}" ] && lsof -ti tcp:1420 >/dev/null 2>&1; then
  echo "✗ port 1420 ถูกใช้อยู่ ปิด dev server ตัวอื่นก่อน" >&2
  exit 1
fi

# preset ของ Lightroom สำหรับขั้นแต่งภาพ (โครงเดียวกับที่ Lightroom Classic ส่งออก ย่อเหลือค่าที่ใช้ทดสอบ)
cat >"$WORK/preset.xmp" <<'XMP'
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:WhiteBalance="As Shot" crs:Exposure2012="+0.40" crs:Shadows2012="+25" crs:Texture="+15"
   crs:Vibrance="+20" crs:LuminanceAdjustmentOrange="+30">
   <crs:Name><rdf:Alt><rdf:li xml:lang="x-default">สว่างใส e2e</rdf:li></rdf:Alt></crs:Name>
   <crs:Look><rdf:Description crs:Name="Adobe Vivid" crs:Amount="1"/></crs:Look>
   <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 12</rdf:li><rdf:li>128, 136</rdf:li><rdf:li>255, 250</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
XMP

# native picker ขับด้วยสคริปต์ไม่ได้: debug build ตอบคำขอ pick_* ตามลำดับนี้แทน
# (1) โฟลเดอร์ที่จะเพิ่ม  (2) ปลายทางส่งออก  (3) ปลายทางส่งออกรอบสอง (ที่เดิม เพื่อทดสอบชื่อชน)  (4) ไฟล์ preset .xmp
export PURI_E2E_PICK="$(native "$PHOTOS")
$(native "$WORK/export")
$(native "$WORK/export")
$(native "$WORK/preset.xmp")"
export PURI_DATA_DIR="$(native "$WORK/library")" PURI_DISABLE_OPEN=1 PURI_DISABLE_FACE_ENGINE=1
# ไม่ให้ฐานของรุ่น Python บนเครื่องผู้พัฒนาทำให้หน้าต่าง "เตรียมแอปให้พร้อมใช้" เปิดขึ้นกลางเทสต์ (ทดสอบแยกใน e2e/setup.sh)
export PURI_LEGACY_DB="$(native "$WORK/no-legacy.db")"
export PURI_E2E_SCRIPT="$(native "$WORK/smoke.js")" PURI_E2E_REPORT="$(native "$REPORT")"
if [ -n "${APP:-}" ]; then
  echo "› เปิด $APP"
  "$APP" >"$WORK/app.log" 2>&1 &
  APP_PID=$!
else
  echo "› เปิดแอป (pnpm tauri dev)"
  pnpm tauri dev >"$WORK/app.log" 2>&1 &
fi

window_id() {
  swift - "$1" <<<'import CoreGraphics
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as! [[String: Any]]
var best = (id: 0, area: 0.0)
let pid = Int(CommandLine.arguments.last ?? "") ?? -1
for w in list where (w[kCGWindowOwnerPID as String] as? Int) == pid {
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
    # แอปที่ผู้ใช้เปิดใช้อยู่มีชื่อเดียวกัน: เลือกหน้าต่างจาก PID ของแอปทดสอบเท่านั้น
    test_pid=${APP_PID:-$(pgrep -f "$ROOT/target/debug/puri-photo-search" | head -1 || true)}
    [ -n "$window" ] || window=$(window_id "$test_pid" || true)
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
  sums=$(find "$PHOTOS" -type f -name "$original" -exec "${SHA[@]}" {} +)
  [ -n "$sums" ] || fail "ไฟล์ส่งออก $name ไม่มีต้นฉบับชื่อ $original"
  sum=$("${SHA[@]}" "$f" | cut -d' ' -f1)
  case "$sums" in *"$sum"*) ;; *) fail "ไฟล์ส่งออก $name ไม่ตรงกับต้นฉบับ" ;; esac
done
[ "$renamed" -ge 1 ] || fail "ไม่มีไฟล์ที่ถูกเติมเลขท้าย ทั้งที่ส่งออกชื่อซ้ำไปที่เดิม"
[ -z "$(find "$WORK/export" -name '.puri-export-*')" ] || fail "เหลือไฟล์ชั่วคราวในปลายทาง"
echo "  · ส่งออก $exported ไฟล์ ตรงกับต้นฉบับทุก byte ($renamed ไฟล์ถูกเติมเลขท้ายแทนการเขียนทับ) ไม่เหลือไฟล์ชั่วคราว"

db="$WORK/library/data/library/library.sqlite"
if command -v sqlite3 >/dev/null 2>&1; then
  q() { sqlite3 "file:$db?mode=ro" "$1"; }
else
  # Windows runner ไม่มี sqlite3: ใช้โมดูลที่มากับ Python อ่านอย่างเดียวแทน
  q() { python -c 'import sqlite3, sys; print(sqlite3.connect(sys.argv[1]).execute(sys.argv[2]).fetchone()[0])' "$(native "$db")" "$1"; }
fi
[ "$(q "SELECT COUNT(*) FROM events")" -ge 1 ] || fail "คลังไม่มีกิจกรรม"
[ "$(q "SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL")" -ge 1 ] || fail "คลังไม่มีคำยืนยันใบหน้า"
[ "$(q "SELECT COUNT(*) FROM photo_edits")" -ge 2 ] || fail "คลังไม่มีการแต่งภาพที่บันทึก"
[ "$(q "SELECT COUNT(*) FROM edit_presets")" = 2 ] || fail "คลังไม่มี preset สองอัน (นำเข้า + บันทึกเอง)"
grep -q 'สว่างใส e2e' "$WORK/preset.xmp" || fail "ไฟล์ preset ต้นทางถูกแก้"
echo "  · คลัง: $(q "SELECT COUNT(*) FROM events") กิจกรรม, $(q "SELECT COUNT(*) FROM photos") รูป, $(q "SELECT COUNT(*) FROM persons") บุคคล, $(q "SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL") คำยืนยัน, $(q "SELECT COUNT(*) FROM photo_edits") รูปที่แต่ง, $(q "SELECT COUNT(*) FROM edit_presets") preset"
echo "✓ e2e ผ่าน"
