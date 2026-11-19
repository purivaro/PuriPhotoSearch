#!/usr/bin/env bash
# ทดสอบการอัปเดตตัวเองของแอปจริง: แอปรุ่นปัจจุบันที่ติดตั้งไว้ต้องพบรุ่นใหม่จาก manifest, ปฏิเสธไฟล์ที่ไม่ตรงลายเซ็น,
# ดาวน์โหลดไฟล์จริง, ติดตั้งเมื่อกดปุ่ม แล้วเปิดขึ้นมาใหม่เป็นรุ่นใหม่
#
#   e2e/update.sh           รันเทสต์ (build สองรอบ ใช้เวลาหลายนาที; ต้องมี desktop session)
#   KEEP=1 e2e/update.sh    ไม่ลบโฟลเดอร์ทำงานหลังจบ
#   REAL_KEY=<ไฟล์กุญแจ> e2e/update.sh   เซ็นด้วยกุญแจจริงและใช้กุญแจสาธารณะใน tauri.conf.json ตามที่เป็น —
#                           พิสูจน์ว่ากุญแจที่เก็บไว้เป็นคู่ของกุญแจสาธารณะที่ฝังในแอป (ทำเมื่อสร้างหรือเปลี่ยนกุญแจ)
#   LIVE=1 e2e/update.sh    ตรวจรุ่นที่เผยแพร่แล้ว (macOS, ออกเครือข่าย): build เดียวที่ตั้งเลขรุ่นเป็น 0.0.1 อ่าน manifest จริงบน GitHub
#                           ดาวน์โหลดไฟล์อัปเดตจริงของรุ่นล่าสุด ตรวจกับกุญแจสาธารณะจริง แล้วกลายเป็นแอปที่เผยแพร่ (เซ็น + notarize)
#                           แอปรอบสองเป็น release build ซึ่งไม่มี hook ของเทสต์ จึงเปิดด้วย HOME ชั่วคราวเพื่อไม่ให้แตะคลังจริง — รันหลังออกรุ่นทุกครั้ง
#
# macOS:   "ติดตั้ง" คือวาง .app ในโฟลเดอร์ชั่วคราว; ตัวอัปเดตสลับ bundle ในที่เดิม
# Windows: ติดตั้งด้วยตัวติดตั้ง NSIS จริงลงโปรไฟล์ของผู้ใช้ (ทับแอปที่ติดตั้งอยู่!) จึงรันได้เฉพาะบน runner ของ CI;
#          ตัวอัปเดตรันตัวติดตั้งรุ่นใหม่แบบ passive ซึ่งเปิดแอปให้ใหม่
#
# ปกติไม่ใช้กุญแจจริงและไม่ออกเครือข่าย: สร้างคู่กุญแจชั่วคราว, build ทั้งสองรุ่นด้วยกุญแจสาธารณะนั้น (--config) และเสิร์ฟ manifest
# จาก 127.0.0.1 — debug build รับปลายทางผ่าน PURI_UPDATE_ENDPOINT (release ไม่มีโค้ดส่วนนี้ และรับเฉพาะ https)
# manifest สร้างด้วย scripts/update-manifest.sh ตัวเดียวกับ workflow Release
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
fail() { echo "✗ $1"; exit 1; }
case "$(uname)" in
  Darwin) OS=mac ;;
  MINGW* | MSYS*) OS=windows ;;
  *) fail "เทสต์นี้รันได้เฉพาะ macOS และ Windows" ;;
esac
[ "$OS" = mac ] || [ "${CI:-}" = true ] || fail "บน Windows เทสต์นี้ติดตั้งแอปทับของจริงในโปรไฟล์ผู้ใช้ จึงรันเฉพาะบน runner ของ CI"

WORK=$(cd "$(mktemp -d)" && pwd -P)
REPORT="$WORK/report.json"
PORT=${PORT:-14270}
NEW=99.0.0
NOTES="ข้อความทดสอบจาก manifest"
cd "$ROOT"
OLD=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json)
[ -n "$OLD" ] || fail "อ่านเลขรุ่นจาก src-tauri/tauri.conf.json ไม่ได้"
LIVE=${LIVE:-0}
if [ "$LIVE" = 1 ]; then
  [ "$(uname)" = Darwin ] || fail "LIVE=1 รันได้เฉพาะ macOS"
  ENDPOINT=$(jq -r '.plugins.updater.endpoints[0]' src-tauri/tauri.conf.json)
  NEW=$(curl -fsSL "$ENDPOINT" | jq -r .version) || fail "อ่าน manifest ที่เผยแพร่ไม่ได้: $ENDPOINT"
  OLD=0.0.1
  REAL_LIBRARY="$HOME/Library/Application Support/com.purivaro.puriphotosearch2/library/library.sqlite"
  real_library_state() { if [ -f "$REAL_LIBRARY" ]; then stat -f '%m %z' "$REAL_LIBRARY"; else echo none; fi; }
  REAL_BEFORE=$(real_library_state)
fi

# Windows (Git Bash): แอปและ PowerShell ต้องได้ path แบบของ Windows
native() { if command -v cygpath >/dev/null 2>&1; then cygpath -w "$1"; else printf '%s' "$1"; fi; }
pwsh_run() { powershell -NoProfile -NonInteractive -Command "$1" | tr -d '\r'; }

if [ "$OS" = mac ]; then
  PYTHON=python3
  kind=app
  bundle="target/debug/bundle/macos"
  artifact=PuriPhotoSearch-mac.app.tar.gz
  installed="$WORK/Applications/Puri Photo Search.app"
  binary="$installed/Contents/MacOS/puri-photo-search"
  built() { echo "$bundle/Puri Photo Search.app"; }
  update_file() { echo "$bundle/Puri Photo Search.app.tar.gz"; }
  install_old() { mkdir -p "$WORK/Applications" && ditto "$(built)" "$installed"; }
  installed_version() { /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$installed/Contents/Info.plist"; }
  running() { pgrep -f "$binary" >/dev/null; }
  stop_app() { pkill -f "$WORK/Applications" 2>/dev/null || true; }
else
  PYTHON=python
  kind=nsis
  bundle="target/debug/bundle/nsis"
  artifact=PuriPhotoSearch-windows-setup.exe
  installed="$(cygpath -u "$LOCALAPPDATA")/Puri Photo Search"
  binary="$installed/puri-photo-search.exe"
  built() { ls "$bundle"/*-setup.exe; }
  update_file() { built; }
  install_old() { pwsh_run "Start-Process -Wait -FilePath '$(native "$(built)")' -ArgumentList '/S'"; }
  # เลขรุ่นที่ตัวติดตั้งบันทึกไว้ (ที่เดียวกับที่ "Apps & features" แสดง)
  installed_version() { pwsh_run "(Get-ItemProperty 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Puri Photo Search' -ErrorAction SilentlyContinue).DisplayVersion"; }
  running() { [ -n "$(pwsh_run "Get-Process puri-photo-search -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Id")" ]; }
  stop_app() { pwsh_run "Stop-Process -Name puri-photo-search -Force -ErrorAction SilentlyContinue" || true; }
fi

cleanup() {
  if [ -n "${SERVER_PID:-}" ]; then kill "$SERVER_PID" 2>/dev/null; wait "$SERVER_PID" 2>/dev/null || true; fi
  stop_app
  if [ "${KEEP:-0}" = 1 ]; then echo "  · โฟลเดอร์ทำงาน: $WORK"; else rm -rf "$WORK" 2>/dev/null || true; fi
}
trap cleanup EXIT

if [ "$LIVE" = 1 ]; then
  echo "› ตรวจรุ่นที่เผยแพร่: $NEW จาก $ENDPOINT"
  config() { printf '{"version":"%s"%s}' "$OLD" "${1:-}"; }
elif [ -n "${REAL_KEY:-}" ]; then
  echo "› ใช้กุญแจจริง กับกุญแจสาธารณะใน tauri.conf.json"
  KEY=$REAL_KEY
  config() { printf '{"productName":"Puri Photo Search"%s}' "${1:-}"; }
else
  echo "› สร้างกุญแจเซ็นชั่วคราว"
  pnpm -s tauri signer generate --ci -w "$(native "$WORK/key")" >/dev/null 2>&1
  KEY="$(native "$WORK/key")"
  config() { printf '{"plugins":{"updater":{"pubkey":"%s"}}%s}' "$(cat "$WORK/key.pub")" "${1:-}"; }
fi
build() { # <ชื่อ log> <config เพิ่ม>
  rm -rf "$bundle"
  pnpm -s tauri build --debug --bundles "$kind" --config "$(config "${2:-}")" >"$WORK/$1.log" 2>&1 || { tail -30 "$WORK/$1.log"; fail "build ไม่ผ่าน ($1)"; }
}

echo "› build รุ่นที่ติดตั้งอยู่ ($OLD) แล้วติดตั้ง"
build build-old
install_old
[ -f "$binary" ] || fail "ไม่พบแอปที่ติดตั้ง: $binary"
[ "$(installed_version)" = "$OLD" ] || fail "รุ่นของแอปที่ติดตั้งไม่ใช่ $OLD (ได้ $(installed_version))"

if [ "$LIVE" != 1 ]; then
  echo "› build รุ่นใหม่ ($NEW) พร้อมไฟล์อัปเดตที่เซ็นแล้ว"
  TAURI_SIGNING_PRIVATE_KEY="$KEY" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    build build-new ",\"version\":\"$NEW\",\"bundle\":{\"createUpdaterArtifacts\":true}"
  mkdir -p "$WORK/serve"
  cp "$(update_file)" "$WORK/good.bin"
  cp "$(update_file).sig" "$WORK/serve/$artifact.sig"
  # รอบแรกเสิร์ฟไฟล์ที่ถูกแก้หนึ่ง byte ภายใต้ลายเซ็นของไฟล์จริง
  { cat "$WORK/good.bin"; printf x; } >"$WORK/serve/$artifact"
  printf '%s' "$NOTES" >"$WORK/notes.txt"
  scripts/update-manifest.sh "$NEW" "http://127.0.0.1:$PORT" "$WORK/serve" "$WORK/notes.txt" >"$WORK/serve/latest.json"
  "$PYTHON" -m http.server "$PORT" --bind 127.0.0.1 --directory "$(native "$WORK/serve")" >"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  ENDPOINT="http://127.0.0.1:$PORT/latest.json"
fi

sed -e "s|__OLD__|$OLD|g" -e "s|__NEW__|$NEW|g" -e "s|__NOTES__|$NOTES|g" -e "s|__LIVE__|$LIVE|g" e2e/update.js >"$WORK/update.js"
node --check "$WORK/update.js" || fail "e2e/update.js มี syntax error"

echo "› เปิดแอปรุ่น $OLD ที่ติดตั้งไว้"
export PURI_DATA_DIR="$(native "$WORK/library")" PURI_DISABLE_OPEN=1 PURI_DISABLE_FACE_ENGINE=1
export PURI_E2E_SCRIPT="$(native "$WORK/update.js")" PURI_E2E_REPORT="$(native "$REPORT")"
export PURI_UPDATE_ENDPOINT="$ENDPOINT"
# release build (แอปรอบสองของ LIVE) ไม่รับ PURI_DATA_DIR: ให้โฟลเดอร์ข้อมูลของมันไปอยู่ใต้ HOME ชั่วคราวแทนคลังจริง
if [ "$LIVE" = 1 ]; then mkdir -p "$WORK/home" && export HOME="$WORK/home"; fi
"$binary" >"$WORK/app.log" 2>&1 &

last=""
relaunched_without_env=0
# LIVE ดาวน์โหลดไฟล์จริง ~20 MB จาก GitHub จึงรอนานกว่า
for _ in $(seq 1 $([ "$LIVE" = 1 ] && echo 2400 || echo 960)); do
  sleep 0.25
  [ -f "$REPORT" ] || continue
  stage=$(sed -n 's/^ "stage": "\(.*\)",\{0,1\}$/\1/p' "$REPORT" | head -1)
  if [ "$stage" = "$last" ]; then
    # Windows: ตัวติดตั้งเปิดแอปใหม่ผ่าน shell ซึ่งอาจไม่ส่งตัวแปรของเทสต์ต่อให้ แอปรอบสองจึงรายงานกลับมาไม่ได้ —
    # ถือว่าถึงปลายทางเมื่อตัวติดตั้งบันทึกรุ่นใหม่แล้วและแอปทำงานอยู่ (ตรวจซ้ำด้านล่าง)
    # LIVE: แอปรอบสองเป็น release build ซึ่งไม่มี hook ของเทสต์เลย — ตัดสินแบบเดียวกัน
    if { [ "$OS" = windows ] || [ "$LIVE" = 1 ]; } && [ "$stage" = confirm ] && [ "$(installed_version)" = "$NEW" ] && running; then
      sleep 5
      [ "$(sed -n 's/^ "stage": "\(.*\)",\{0,1\}$/\1/p' "$REPORT" | head -1)" = done ] || relaunched_without_env=1
      last=done
      echo "  · done"
      break
    fi
    continue
  fi
  last=$stage
  echo "  · $stage"
  case "$stage" in
    rejected)
      [ "$(installed_version)" = "$OLD" ] || fail "แอปถูกแก้ทั้งที่ไฟล์ไม่ผ่านลายเซ็น"
      cp "$WORK/good.bin" "$WORK/serve/$artifact"
      ;;
    done | FAILED) break ;;
  esac
done

mkdir -p target
cp "$REPORT" target/e2e-update-report.json 2>/dev/null || true
if [ "$last" != done ]; then
  echo "✗ เทสต์อัปเดตไม่ผ่าน (ขั้นสุดท้าย: ${last:-ยังไม่เริ่ม}) — ดู target/e2e-update-report.json"
  tail -5 "$WORK/app.log" 2>/dev/null || true
  [ -f "$REPORT" ] && sed -n '/"error"/p' "$REPORT"
  [ "$OS" = windows ] && echo "  รุ่นที่ติดตั้งอยู่ตอนนี้: $(installed_version)"
  exit 1
fi

# ── ตรวจผลบนดิสก์ ไม่เชื่อแค่สิ่งที่ UI แสดง
[ "$(installed_version)" = "$NEW" ] || fail "แอปที่ติดตั้งยังไม่ใช่รุ่น $NEW (ได้ $(installed_version))"
running || fail "แอปไม่ได้เปิดขึ้นมาใหม่"
[ -f "$WORK/library/data/library/library.sqlite" ] || fail "คลังของเทสต์หายหลังอัปเดต"
if [ "$LIVE" = 1 ]; then
  # แอปที่ได้ต้องเป็นตัวที่เผยแพร่: เซ็นด้วย Developer ID, notarize แล้ว และ Gatekeeper รับ
  codesign --verify --deep --strict "$installed" || fail "ลายเซ็นของแอปที่อัปเดตมาไม่สมบูรณ์"
  xcrun stapler validate "$installed" >/dev/null || fail "แอปที่อัปเดตมาไม่มีใบ notarize"
  spctl -a -vv "$installed" 2>&1 | grep -q "Notarized Developer ID" || fail "Gatekeeper ไม่รับแอปที่อัปเดตมา"
  lipo -archs "$binary" | grep -q "x86_64" || fail "แอปที่อัปเดตมาไม่ใช่ universal"
  [ -d "$WORK/home/Library/Application Support/com.purivaro.puriphotosearch2" ] || fail "แอปรุ่นใหม่ไม่ได้ใช้โฟลเดอร์ข้อมูลใต้ HOME ชั่วคราว"
  [ "$(real_library_state)" = "$REAL_BEFORE" ] || fail "คลังจริงของผู้ใช้เปลี่ยนระหว่างเทสต์ (ถ้าไม่ได้ใช้แอปจริงอยู่ระหว่างนั้น แปลว่าเทสต์ไปแตะคลังจริง)"
  echo "  · แอปในตำแหน่งเดิมกลายเป็นรุ่น $NEW ที่เผยแพร่ (เซ็น + notarize, Gatekeeper รับ, universal) และเปิดขึ้นมาใหม่เอง"
  echo "  · แอปรุ่นใหม่ใช้โฟลเดอร์ข้อมูลใต้ HOME ชั่วคราว คลังจริงไม่เปลี่ยน"
  echo "  ! แอปรอบสองเป็น release build จึงไม่ได้รายงานเลขรุ่นจากในแอป (ยืนยันจาก Info.plist และ process)"
elif [ "$OS" = mac ]; then
  # debug bundle ไม่ได้เซ็น จึงเทียบกับผล build ตรง ๆ (ลายเซ็นของรุ่นจริงตรวจใน workflow Release)
  diff -r "$(built)" "$installed" >/dev/null || fail "แอปที่ติดตั้งไม่ตรงกับรุ่นใหม่ที่ build"
  [ -z "$(find "$WORK/Applications" -mindepth 1 -maxdepth 1 ! -name "Puri Photo Search.app")" ] || fail "เหลือไฟล์ชั่วคราวของตัวอัปเดตข้างแอป"
  echo "  · แอปในตำแหน่งเดิมเป็นรุ่น $NEW ตรงกับผล build ทุกไฟล์, เปิดขึ้นมาใหม่เอง, คลังอยู่ครบ, ไม่เหลือไฟล์ชั่วคราว"
elif [ "$relaunched_without_env" = 1 ]; then
  echo "  · ตัวติดตั้งบันทึกรุ่น $NEW และแอปเปิดขึ้นมาใหม่เอง, คลังอยู่ครบ"
  echo "  ! แอปที่เปิดใหม่ไม่ได้รับตัวแปรของเทสต์ จึงไม่ได้ยืนยันเลขรุ่นจากในแอปรอบสอง (ยืนยันจากเลขรุ่นที่ตัวติดตั้งบันทึก)"
else
  echo "  · ตัวติดตั้งบันทึกรุ่น $NEW, แอปเปิดขึ้นมาใหม่เองและรายงานรุ่นใหม่จากในแอป, คลังอยู่ครบ"
fi
echo "✓ เทสต์อัปเดตผ่าน: $OLD → $NEW"
