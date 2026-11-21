import type { EditMask, EditSettings, MaskArea, UnitRect } from '@/lib/ipc/types';
import { VIGNETTE_NEUTRAL, applyToPixels, isNeutral } from './adjust';
import { fromFrame, toFrame } from './editGeometry';
import { applyLocal, hasLocal } from './local';

/**
 * ปรับเฉพาะส่วน (DECISIONS ข้อ 28) — สูตรเดียวกับ `crates/core/src/masks.rs` (ค่าอ้างอิงร่วม MASK_EXPECTED)
 * ทำหลังแสงสีของทั้งภาพ: ปรับส่วนที่ครอบบริเวณด้วยค่าของ Mask แล้วผสมกับภาพเดิมตามน้ำหนักของ Mask
 * พิกัดของบริเวณเป็นสัดส่วนบนภาพเดิม (ก่อนตัด/หมุน)
 */

// วงรีรอบใบหน้า: ครึ่งบนสั้น (ไม่ล้นขึ้นไปบนฉากหลังเหนือศีรษะ) ครึ่งล่างยาว (คลุมคางและคอ)
const FACE_RX = 0.62;
const FACE_UP = 0.62;
const FACE_DOWN = 0.85;
const FACE_DROP = 0.06;
const FACE_SOFT: [number, number] = [0.75, 1.15];
const SKIN_CB: [number, number, number, number] = [77, 127, 8, 8];
const SKIN_CR: [number, number, number, number] = [135, 173, 5, 8];
const MARGIN = 0.03;
const TONE_INNER = 0.45;
const TONE_MIN_SAMPLES = 12;
const TONE_NEAR = 10;
const TONE_FAR = 20;
export const MAX_MASKS = 10;

/** x² แบบคูณตรง ๆ (ผลเท่ากับ powi(2) ของ Rust ทุกบิต — `x ** 2` ของ JavaScript ไม่รับประกัน) */
const sq = (x: number) => x * x;

const smoothstep = (a: number, b: number, x: number) => {
  const t = Math.min(Math.max((x - a) / (b - a), 0), 1);
  return t * t * (3 - 2 * t);
};

export const maskIsActive = (m: EditMask) => m.amount > 0 && !isNeutral(m.adjust);

const chroma = (r: number, g: number, b: number): [number, number] => [128 - 0.168736 * r - 0.331264 * g + 0.5 * b, 128 + 0.5 * r - 0.418688 * g - 0.081312 * b];

/** ความเป็นสีผิว 0–1 จาก Cb/Cr ของ YCbCr (สีจีวรส้มสด สีเทา/ขาวเป็นกลาง อยู่นอกช่วง) */
export function skinWeight(r: number, g: number, b: number): number {
  const [cb, cr] = chroma(r, g, b);
  const inside = (v: number, [lo, hi, below, above]: [number, number, number, number]) => smoothstep(lo - below, lo, v) * (1 - smoothstep(hi, hi + above, v));
  return inside(cb, SKIN_CB) * inside(cr, SKIN_CR);
}

interface Oval {
  c: [number, number];
  rx: number;
  up: number;
  down: number;
}

function ovalOf(b: [number, number, number, number]): Oval {
  return { c: [b[0] + b[2] / 2, b[1] + b[3] / 2 + b[3] * FACE_DROP], rx: b[2] * FACE_RX, up: b[3] * FACE_UP, down: b[3] * FACE_DOWN };
}

/** ระยะจากศูนย์กลางเทียบกับรัศมี (1 = ขอบวงรี) */
function ovalDistance(o: Oval, u: number, v: number): number {
  const ry = v < o.c[1] ? o.up : o.down;
  return Math.sqrt(sq((u - o.c[0]) / o.rx) + sq((v - o.c[1]) / ry));
}

/** มุมทั้งสี่ของกรอบที่ครอบวงรีที่ระยะ k */
function ovalCorners(o: Oval, k: number): [number, number][] {
  const [x0, x1] = [o.c[0] - o.rx * k, o.c[0] + o.rx * k];
  const [y0, y1] = [o.c[1] - o.up * k, o.c[1] + o.down * k];
  return [
    [x0, y0],
    [x1, y0],
    [x0, y1],
    [x1, y1],
  ];
}

/** ใกล้สีผิวของคนนั้นแค่ไหน (1 = ใกล้ · 0 = สีอื่นที่บังเอิญอยู่ในช่วงสีผิว) — ไม่รู้สีผิว = 1 */
function toneMatch(tone: [number, number] | null, r: number, g: number, b: number): number {
  if (!tone) return 1;
  const [cb, cr] = chroma(r, g, b);
  return 1 - smoothstep(TONE_NEAR, TONE_FAR, Math.sqrt(sq(cb - tone[0]) + sq(cr - tone[1])));
}

/** น้ำหนักของบริเวณที่จุด (u, v) บนภาพเดิม สำหรับพิกเซลสี rgb (ก่อนกลับด้าน/ความแรง) · tones = สีผิวของแต่ละหน้า */
export function areaWeight(area: MaskArea, tones: ([number, number] | null)[], u: number, v: number, r: number, g: number, b: number): number {
  let w = 0;
  area.boxes.forEach((box, i) => {
    let f = 1 - smoothstep(FACE_SOFT[0], FACE_SOFT[1], ovalDistance(ovalOf(box), u, v));
    if (f <= w) return;
    if (area.skin) f *= skinWeight(r, g, b) * toneMatch(tones[i] ?? null, r, g, b);
    w = Math.max(w, f);
  });
  return w;
}

/** ภาพที่วาดบน canvas: ขนาด และส่วน `view` ของกรอบหลังปรับเอียง (ยังไม่ตัด) ที่ canvas แสดง */
export interface MaskCanvas {
  width: number;
  height: number;
  view: UnitRect;
  /** ขนาดภาพเดิม (หลังหมุนตาม EXIF) — ใช้แปลงพิกัด */
  sourceW: number;
  sourceH: number;
  settings: EditSettings;
}

/** จุดบน canvas → จุดบนภาพเดิม */
function toSource(c: MaskCanvas, x: number, y: number) {
  const fx = c.view.x + ((x + 0.5) / c.width) * c.view.w;
  const fy = c.view.y + ((y + 0.5) / c.height) * c.view.h;
  return fromFrame(c.sourceW, c.sourceH, c.settings, fx, fy);
}

/** สีผิวของแต่ละหน้า: Cb/Cr เฉลี่ยของพิกเซลสีผิวกลางใบหน้าบน canvas — หาไม่พอ = null */
function faceTones(px: Uint8ClampedArray, c: MaskCanvas, area: MaskArea): ([number, number] | null)[] {
  if (!area.skin) return area.boxes.map(() => null);
  return area.boxes.map((box) => {
    const oval = ovalOf(box);
    let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
    for (const [u, v] of ovalCorners(oval, TONE_INNER)) {
      const f = toFrame(c.sourceW, c.sourceH, c.settings, u, v);
      const px2 = ((f.x - c.view.x) / c.view.w) * c.width;
      const py2 = ((f.y - c.view.y) / c.view.h) * c.height;
      [x0, y0, x1, y1] = [Math.min(x0, px2), Math.min(y0, py2), Math.max(x1, px2), Math.max(y1, py2)];
    }
    const ax = Math.min(Math.max(Math.floor(x0), 0), c.width);
    const bx = Math.min(Math.max(Math.ceil(x1), 0), c.width);
    const ay = Math.min(Math.max(Math.floor(y0), 0), c.height);
    const by = Math.min(Math.max(Math.ceil(y1), 0), c.height);
    let [sb, sr, n] = [0, 0, 0];
    for (let y = ay; y < by; y += 1) {
      for (let x = ax; x < bx; x += 1) {
        const s = toSource(c, x, y);
        if (ovalDistance(oval, s.x, s.y) > TONE_INNER) continue;
        const i = (y * c.width + x) * 4;
        if (skinWeight(px[i]!, px[i + 1]!, px[i + 2]!) > 0.5) {
          const [cb, cr] = chroma(px[i]!, px[i + 1]!, px[i + 2]!);
          sb += cb;
          sr += cr;
          n += 1;
        }
      }
    }
    return n >= TONE_MIN_SAMPLES ? [sb / n, sr / n] : null;
  });
}

/** กรอบบน canvas (พิกเซล) ที่ครอบส่วนที่มีน้ำหนัก + ขอบเผื่อ · null = ไม่มีส่วนใดอยู่ในภาพ */
function bounds(area: MaskArea, c: MaskCanvas, long: number): [number, number, number, number] | null {
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const box of area.boxes) {
    for (const [u, v] of ovalCorners(ovalOf(box), FACE_SOFT[1])) {
      const f = toFrame(c.sourceW, c.sourceH, c.settings, u, v);
      const px = ((f.x - c.view.x) / c.view.w) * c.width;
      const py = ((f.y - c.view.y) / c.view.h) * c.height;
      [x0, y0, x1, y1] = [Math.min(x0, px), Math.min(y0, py), Math.max(x1, px), Math.max(y1, py)];
    }
  }
  const margin = MARGIN * long;
  const ax = Math.floor(Math.min(Math.max(x0 - margin, 0), c.width));
  const ay = Math.floor(Math.min(Math.max(y0 - margin, 0), c.height));
  const bx = Math.ceil(Math.min(Math.max(x1 + margin, 0), c.width));
  const by = Math.ceil(Math.min(Math.max(y1 + margin, 0), c.height));
  return bx > ax && by > ay ? [ax, ay, bx, by] : null;
}

/**
 * ปรับเฉพาะส่วนบนพิกเซลของ canvas (RGBA) ในที่ · `long` = ด้านยาวของภาพที่แต่งทั้งกรอบเป็นพิกเซลของ canvas
 * `overlay` = แสดงบริเวณของ Mask นี้เป็นสีแดงทับ (ให้เห็นว่าเลือกตรงไหน) แทนการปรับ
 */
export function applyMasks(px: Uint8ClampedArray, c: MaskCanvas, masks: EditMask[], long: number, overlay: string | null = null): void {
  for (const mask of masks) {
    const show = overlay === mask.id;
    if (!show && !maskIsActive(mask)) continue;
    const region = mask.invert ? ([0, 0, c.width, c.height] as [number, number, number, number]) : bounds(mask.area, c, long);
    if (!region) continue;
    const [x0, y0, x1, y1] = region;
    const rw = x1 - x0;
    const rh = y1 - y0;
    let adjusted: Uint8ClampedArray | null = null;
    if (!show) {
      adjusted = new Uint8ClampedArray(rw * rh * 4);
      for (let y = 0; y < rh; y += 1) adjusted.set(px.subarray(((y0 + y) * c.width + x0) * 4, ((y0 + y) * c.width + x1) * 4), y * rw * 4);
      // Vignette เป็นของกรอบภาพทั้งภาพ ไม่ใช่ของบริเวณ — ไม่ใช้ในบริเวณ (เหมือน masks.rs)
      applyToPixels(adjusted, { ...mask.adjust, texture: 0, clarity: 0, dehaze: 0, vignette: VIGNETTE_NEUTRAL }, undefined, 'display');
      if (hasLocal(mask.adjust)) applyLocal(adjusted, rw, rh, 4, long, mask.adjust);
    }
    const amount = mask.amount / 100;
    const tones = faceTones(px, c, mask.area);
    for (let y = 0; y < rh; y += 1) {
      for (let x = 0; x < rw; x += 1) {
        const i = ((y0 + y) * c.width + x0 + x) * 4;
        const s = toSource(c, x0 + x, y0 + y);
        let t = areaWeight(mask.area, tones, s.x, s.y, px[i]!, px[i + 1]!, px[i + 2]!);
        if (mask.invert) t = 1 - t;
        if (show) {
          // บริเวณที่เลือก: แดงโปร่ง ตามน้ำหนัก
          const k = t * 0.55;
          px[i] = Math.round(px[i]! + (230 - px[i]!) * k);
          px[i + 1] = Math.round(px[i + 1]! + (40 - px[i + 1]!) * k);
          px[i + 2] = Math.round(px[i + 2]! + (60 - px[i + 2]!) * k);
          continue;
        }
        t *= amount;
        if (t <= 0 || !adjusted) continue;
        const a = (y * rw + x) * 4;
        for (let ch = 0; ch < 3; ch += 1) px[i + ch] = Math.round(px[i + ch]! + (adjusted[a + ch]! - px[i + ch]!) * t);
      }
    }
  }
}
