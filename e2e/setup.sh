#!/usr/bin/env bash
# ทดสอบหน้าต่าง "เตรียมแอปให้พร้อมใช้" (S01) ในหน้าต่างแอปจริง: คลังว่าง ยังไม่มีตัววิเคราะห์ใบหน้า
# และมีฐานของ PuriPhotoSearch รุ่น Python ในเครื่อง — แอปต้องเสนอทั้งสองอย่างเอง จำว่าผู้ใช้ปิดแล้ว และนำรายชื่อเข้าได้
#
#   e2e/setup.sh            รันเทสต์ (macOS; ต้องมี sqlite3 และ desktop session)
#   SHOTS=1 e2e/setup.sh    เก็บภาพหน้าต่างทุกขั้นไว้ที่ screenshots/e2e-setup/
#   KEEP=1 e2e/setup.sh     ไม่ลบโฟลเดอร์ทำงานหลังจบ
#
# ใช้คลังชั่วคราวและฐานรุ่นเดิมจำลอง (PURI_LEGACY_DB) ไม่แตะคลังจริงหรือฐานจริงของผู้ใช้ และไม่ออกเครือข่าย
# (ตรวจแค่ว่าปุ่มดาวน์โหลดตัววิเคราะห์ใบหน้าถูกเสนอ ไม่ได้กดดาวน์โหลด 288 MB)
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
fail() { echo "✗ $1"; exit 1; }
command -v sqlite3 >/dev/null 2>&1 || fail "ต้องมี sqlite3 เพื่อสร้างฐานรุ่นเดิมจำลอง"
WORK=$(cd "$(mktemp -d)" && pwd -P)
REPORT="$WORK/report.json"
cd "$ROOT"

cleanup() {
  pkill -f "$ROOT/target/debug/puri-photo-search" 2>/dev/null || true
  lsof -ti tcp:1420 2>/dev/null | xargs kill 2>/dev/null || true
  if [ "${KEEP:-0}" = 1 ]; then echo "  · โฟลเดอร์ทำงาน: $WORK"; else rm -rf "$WORK"; fi
}
trap cleanup EXIT

echo "› สร้างฐานรุ่นเดิมจำลอง (3 คน ชื่อซ้ำ 1 คู่ 1 กลุ่ม ใบหน้าตัวอย่าง 4 ใบ)"
cargo run -q -p puri-core --example make_fixtures -- "$WORK/photos" >/dev/null
face=$(find "$WORK/photos" -name '*.jpg' -size +1k | sort | head -1)
[ -n "$face" ] || fail "ไม่มีไฟล์รูปสำหรับใช้เป็นใบหน้าตัวอย่าง"
LEGACY="$WORK/legacy/photosearch.db"
mkdir -p "$WORK/legacy"
sqlite3 "$LEGACY" <<SQL
CREATE TABLE persons (id INTEGER PRIMARY KEY, name TEXT NOT NULL, created_at TEXT, group_name TEXT);
CREATE TABLE person_embeddings (id INTEGER PRIMARY KEY, person_id INTEGER, photo_path TEXT, embedding BLOB, thumbnail BLOB, is_primary INTEGER);
CREATE TABLE event_folders (id INTEGER PRIMARY KEY, folder_path TEXT);
CREATE TABLE photos (id INTEGER PRIMARY KEY, file_path TEXT);
CREATE TABLE faces (id INTEGER PRIMARY KEY, photo_id INTEGER, embedding BLOB);
INSERT INTO persons VALUES (1, 'คุณกานต์', '2026-03-09 10:00:00', 'ทีมงาน'), (2, 'สมชาย', '2026-03-09 10:01:00', NULL), (3, 'สมชาย', '2026-03-10 09:00:00', 'ทีมงาน');
INSERT INTO person_embeddings(person_id, photo_path, embedding, thumbnail, is_primary) VALUES
  (1, '/gone.jpg', zeroblob(2048), readfile('$face'), 1), (1, '/gone.jpg', zeroblob(2048), readfile('$face'), 0),
  (2, '/gone.jpg', zeroblob(2048), readfile('$face'), 1), (3, '/gone.jpg', zeroblob(2048), readfile('$face'), 1);
SQL
legacy_sum=$(shasum -a 256 "$LEGACY" | cut -d' ' -f1)

node --check e2e/setup.js || fail "e2e/setup.js มี syntax error"
lsof -ti tcp:1420 >/dev/null 2>&1 && fail "port 1420 ถูกใช้อยู่ ปิด dev server ตัวอื่นก่อน"

# ไม่ตั้ง PURI_DISABLE_FACE_ENGINE: ตัววิเคราะห์ใบหน้าต้องอยู่ในสถานะ "ยังไม่ได้ดาวน์โหลด" (โมเดลอยู่ใต้ PURI_DATA_DIR ซึ่งว่าง)
export PURI_DATA_DIR="$WORK/library" PURI_DISABLE_OPEN=1 PURI_LEGACY_DB="$LEGACY"
export PURI_E2E_SCRIPT="$ROOT/e2e/setup.js" PURI_E2E_REPORT="$REPORT"
# ตัวหาหน้าต่างของแอปสำหรับถ่ายภาพ: compile ไว้ก่อนเปิดแอป เพื่อให้ภาพแรกทันขั้นแรก
if [ "${SHOTS:-0}" = 1 ]; then
  cat >"$WORK/winid.swift" <<'SWIFT'
import CoreGraphics
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as! [[String: Any]]
var best = (id: 0, area: 0.0)
let pid = Int(CommandLine.arguments.last ?? "") ?? -1
for w in list where (w[kCGWindowOwnerPID as String] as? Int) == pid {
  let b = w[kCGWindowBounds as String] as? [String: Double] ?? [:]
  let area = (b["Width"] ?? 0) * (b["Height"] ?? 0)
  if area > best.area { best = (w[kCGWindowNumber as String] as! Int, area) }
}
if best.id != 0 { print(best.id) }
SWIFT
  swiftc -O -o "$WORK/winid" "$WORK/winid.swift" 2>/dev/null || fail "compile ตัวหาหน้าต่างไม่ได้ (ต้องมี Xcode command line tools)"
fi

echo "› เปิดแอป (pnpm tauri dev)"
pnpm tauri dev >"$WORK/app.log" 2>&1 &

[ "${SHOTS:-0}" = 1 ] && rm -f "${ROOT:?}"/screenshots/e2e-setup/*.png
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
    mkdir -p "$ROOT/screenshots/e2e-setup"
    sleep 0.5
    # แอปที่ผู้ใช้เปิดใช้อยู่มีชื่อเดียวกัน: เลือกหน้าต่างจาก PID ของแอปทดสอบเท่านั้น
    [ -n "$window" ] || window=$("$WORK/winid" "$(pgrep -f "$ROOT/target/debug/puri-photo-search" | head -1 || true)" || true)
    [ -n "$window" ] && screencapture -x -o -l "$window" "$ROOT/screenshots/e2e-setup/$(printf '%02d' $shot)-$stage.png" 2>/dev/null || true
    shot=$((shot + 1))
  fi
  if [ "$stage" = done ] || [ "$stage" = FAILED ]; then break; fi
done

mkdir -p target
cp "$REPORT" target/e2e-setup-report.json 2>/dev/null || true
if [ "$last" != done ]; then
  echo "✗ e2e ไม่ผ่าน (ขั้นสุดท้าย: ${last:-ยังไม่เริ่ม}) — ดู target/e2e-setup-report.json"
  tail -5 "$WORK/app.log" 2>/dev/null || true
  [ -f "$REPORT" ] && sed -n '/"error"/,/^ *[}\]]/p' "$REPORT" | head -12
  exit 1
fi

# ── ตรวจผลบนดิสก์ ไม่เชื่อแค่สิ่งที่ UI แสดง
[ "$(shasum -a 256 "$LEGACY" | cut -d' ' -f1)" = "$legacy_sum" ] || fail "ฐานของรุ่นเดิมถูกแก้"
[ -z "$(find "$WORK/legacy" -type f ! -name photosearch.db)" ] || fail "มีไฟล์เกิดขึ้นข้างฐานของรุ่นเดิม"
echo "  · ฐานของรุ่นเดิมเหมือนเดิมทุก byte ไม่มีไฟล์ -wal/-shm เกิดข้าง ๆ"
q() { sqlite3 "file:$WORK/library/data/library/library.sqlite?mode=ro" "$1"; }
[ "$(q "SELECT COUNT(*) FROM persons")" = 3 ] || fail "คลังไม่ได้มี 3 บุคคล"
[ "$(q "SELECT COUNT(*) FROM person_reference_samples WHERE source = 'legacy_python'")" = 4 ] || fail "ใบหน้าตัวอย่างไม่ครบ 4 ใบ"
[ "$(q "SELECT COUNT(*) FROM ui_preferences WHERE key = 'setup_dismissed'")" = 1 ] || fail "ไม่ได้จำว่าผู้ใช้ปิดหน้าต่างแล้ว"
[ ! -d "$WORK/library/data/library/models" ] || [ -z "$(find "$WORK/library/data/library/models" -type f)" ] || fail "มีไฟล์โมเดลถูกดาวน์โหลดทั้งที่ไม่ได้กด"
echo "  · คลัง: 3 บุคคล, ใบหน้าตัวอย่าง 4 ใบ, จำว่าปิดหน้าต่างแล้ว, ไม่มีการดาวน์โหลดโมเดล"
echo "✓ e2e ผ่าน"
