import type { EditAdjust as Adjust } from '@/lib/ipc/types';

/**
 * ปรับที่ต้องดูพิกเซลรอบข้าง: Texture, Clarity, Dehaze (DECISIONS ข้อ 27) — สูตรเดียวกับ `crates/core/src/local.rs`
 * (ค่าอ้างอิงร่วม LOCAL_EXPECTED) ทำหลังขั้นต่อพิกเซลทั้งหมด · รัศมีเป็นสัดส่วนของด้านยาวของภาพทั้งกรอบ
 * ตัวอย่างสดบนจอจึงได้ผลแบบเดียวกับภาพเต็มขนาดที่ Rust สร้าง
 */

const TEXTURE_RADIUS = 0.0035;
const TEXTURE_EPS = 0.004;
const CLARITY_RADIUS = 0.015;
const CLARITY_EPS = 0.02;
const DEHAZE_BLOCK = 0.01;
const DEHAZE_EPS = 0.001;
const MIN_TRANSMISSION = 0.1;

// ค่าที่บันทึกก่อนมีขั้นนี้ไม่มีช่องเหล่านี้: ถือเป็น 0 (ไม่ให้ NaN ทำภาพดำทั้งภาพ)
const amount = (v: number | undefined) => (Number.isFinite(v) ? (v as number) : 0);

export const hasLocal = (a: Adjust) => amount(a.texture) !== 0 || amount(a.clarity) !== 0 || amount(a.dehaze) !== 0;

const clamp01 = (v: number) => Math.min(Math.max(v, 0), 1);
const toByte = (v: number) => Math.round(clamp01(v) * 255);
const luma = (px: ArrayLike<number>, i: number) => (0.2126 * px[i]! + 0.7152 * px[i + 1]! + 0.0722 * px[i + 2]!) / 255;
const radius = (fraction: number, long: number) => Math.max(1, Math.round(fraction * long));

/** ค่าเฉลี่ยในหน้าต่าง (2r+1)² — ที่ขอบภาพนับเฉพาะพิกเซลที่อยู่ในภาพ */
function boxMean(src: Float32Array, w: number, h: number, r: number): Float32Array {
  const prefix = new Float64Array(Math.max(w, h) + 1);
  const rows = new Float32Array(w * h);
  for (let y = 0; y < h; y += 1) {
    for (let x = 0; x < w; x += 1) prefix[x + 1] = prefix[x]! + src[y * w + x]!;
    for (let x = 0; x < w; x += 1) {
      const a = Math.max(0, x - r);
      const b = Math.min(w, x + r + 1);
      rows[y * w + x] = (prefix[b]! - prefix[a]!) / (b - a);
    }
  }
  const out = new Float32Array(w * h);
  for (let x = 0; x < w; x += 1) {
    for (let y = 0; y < h; y += 1) prefix[y + 1] = prefix[y]! + rows[y * w + x]!;
    for (let y = 0; y < h; y += 1) {
      const a = Math.max(0, y - r);
      const b = Math.min(h, y + r + 1);
      out[y * w + x] = (prefix[b]! - prefix[a]!) / (b - a);
    }
  }
  return out;
}

/** ย่อด้วยค่าเฉลี่ยของบล็อก s×s */
function shrink(src: Float32Array, w: number, h: number, s: number): [Float32Array, number, number] {
  if (s <= 1) return [src, w, h];
  const wl = Math.ceil(w / s);
  const hl = Math.ceil(h / s);
  const sum = new Float64Array(wl * hl);
  const n = new Uint32Array(wl * hl);
  for (let y = 0; y < h; y += 1) {
    for (let x = 0; x < w; x += 1) {
      const k = Math.floor(y / s) * wl + Math.floor(x / s);
      sum[k] = sum[k]! + src[y * w + x]!;
      n[k] = n[k]! + 1;
    }
  }
  return [Float32Array.from(sum, (v, k) => v / n[k]!), wl, hl];
}

/** สัมประสิทธิ์ของ guided filter ที่ความละเอียดต่ำ (ค่าผลที่พิกเซลเต็มขนาด = a·guide + b) */
interface Coeffs {
  a: Float32Array;
  b: Float32Array;
  w: number;
  h: number;
  s: number;
}

function guided(guide: Float32Array, input: Float32Array, w: number, h: number, r: number, eps: number, s: number): Coeffs {
  const meanI = boxMean(guide, w, h, r);
  const meanP = boxMean(input, w, h, r);
  const ip = new Float32Array(w * h);
  const ii = new Float32Array(w * h);
  for (let k = 0; k < w * h; k += 1) {
    ip[k] = guide[k]! * input[k]!;
    ii[k] = guide[k]! * guide[k]!;
  }
  const meanIp = boxMean(ip, w, h, r);
  const meanIi = boxMean(ii, w, h, r);
  const a = new Float32Array(w * h);
  const b = new Float32Array(w * h);
  for (let k = 0; k < w * h; k += 1) {
    const cov = meanIp[k]! - meanI[k]! * meanP[k]!;
    const variance = meanIi[k]! - meanI[k]! * meanI[k]!;
    a[k] = cov / (variance + eps);
    b[k] = meanP[k]! - a[k]! * meanI[k]!;
  }
  return { a: boxMean(a, w, h, r), b: boxMean(b, w, h, r), w, h, s };
}

/** a, b ที่พิกเซล (x, y) ของภาพเต็มขนาด (ขยายแบบ bilinear) */
function coeffAt(c: Coeffs, x: number, y: number): [number, number] {
  const fx = Math.min(Math.max((x + 0.5) / c.s - 0.5, 0), c.w - 1);
  const fy = Math.min(Math.max((y + 0.5) / c.s - 0.5, 0), c.h - 1);
  const x0 = Math.floor(fx);
  const y0 = Math.floor(fy);
  const x1 = Math.min(x0 + 1, c.w - 1);
  const y1 = Math.min(y0 + 1, c.h - 1);
  const tx = fx - x0;
  const ty = fy - y0;
  const lerp = (v: Float32Array) => {
    const top = v[y0 * c.w + x0]! * (1 - tx) + v[y0 * c.w + x1]! * tx;
    const bottom = v[y1 * c.w + x0]! * (1 - tx) + v[y1 * c.w + x1]! * tx;
    return top * (1 - ty) + bottom * ty;
  };
  return [lerp(c.a), lerp(c.b)];
}

/** เบลอแบบรักษาขอบของความสว่าง ที่รัศมี `fraction` ของด้านยาว (คิดที่ความละเอียด 1/s) */
function smoothLuma(y: Float32Array, w: number, h: number, long: number, fraction: number, eps: number): Coeffs {
  const r = radius(fraction, long);
  const s = Math.max(1, Math.floor(r / 4));
  const [low, wl, hl] = shrink(y, w, h, s);
  const rl = Math.max(1, Math.round(r / s));
  return guided(low, low, wl, hl, rl, eps, s);
}

function detail(px: Uint8ClampedArray | Uint8Array, w: number, h: number, stride: number, long: number, texture: number, clarity: number) {
  const y = new Float32Array(w * h);
  for (let i = 0; i < w * h; i += 1) y[i] = luma(px, i * stride);
  const tex = texture !== 0 ? smoothLuma(y, w, h, long, TEXTURE_RADIUS, TEXTURE_EPS) : null;
  const cla = clarity !== 0 ? smoothLuma(y, w, h, long, CLARITY_RADIUS, CLARITY_EPS) : null;
  for (let row = 0; row < h; row += 1) {
    for (let col = 0; col < w; col += 1) {
      const i = row * w + col;
      const l = y[i]!;
      let delta = 0;
      if (tex) {
        const [a, b] = coeffAt(tex, col, row);
        delta += texture * (l - (a * l + b));
      }
      if (cla) {
        const [a, b] = coeffAt(cla, col, row);
        const mid = 1 - (2 * l - 1) * (2 * l - 1);
        delta += clarity * (l - (a * l + b)) * mid;
      }
      const p = i * stride;
      for (let c = 0; c < 3; c += 1) px[p + c] = toByte(px[p + c]! / 255 + delta);
    }
  }
}

function dehaze(px: Uint8ClampedArray | Uint8Array, w: number, h: number, stride: number, long: number, amount: number) {
  const s = radius(DEHAZE_BLOCK, long);
  const wl = Math.ceil(w / s);
  const hl = Math.ceil(h / s);
  const dark = new Float32Array(wl * hl).fill(1);
  const sum = new Float64Array(wl * hl * 4);
  const n = new Uint32Array(wl * hl);
  for (let row = 0; row < h; row += 1) {
    for (let col = 0; col < w; col += 1) {
      const i = (row * w + col) * stride;
      const k = Math.floor(row / s) * wl + Math.floor(col / s);
      dark[k] = Math.min(dark[k]!, Math.min(px[i]!, px[i + 1]!, px[i + 2]!) / 255);
      for (let c = 0; c < 3; c += 1) sum[k * 4 + c] = sum[k * 4 + c]! + px[i + c]! / 255;
      sum[k * 4 + 3] = sum[k * 4 + 3]! + Math.fround(luma(px, i));
      n[k] = n[k]! + 1;
    }
  }
  const mean = (k: number, c: number) => sum[k * 4 + c]! / n[k]!;
  // สีของหมอก: ค่าเฉลี่ยของ 1% ของบล็อกที่ dark channel สูงสุด (เท่ากันเรียงตามตำแหน่ง)
  const order = Array.from({ length: wl * hl }, (_, k) => k).sort((a, b) => dark[b]! - dark[a]!);
  const top = Math.max(1, Math.floor((wl * hl) / 100));
  const air = [0, 0, 0];
  for (const k of order.slice(0, top)) for (let c = 0; c < 3; c += 1) air[c]! += mean(k, c);
  for (let c = 0; c < 3; c += 1) air[c] = Math.max(air[c]! / top, 0.05);
  if (amount < 0) {
    const t = -amount * 0.5;
    for (let i = 0; i < w * h; i += 1) {
      for (let c = 0; c < 3; c += 1) {
        const v = px[i * stride + c]! / 255;
        px[i * stride + c] = toByte(v + (air[c]! - v) * t);
      }
    }
    return;
  }
  const norm = new Float32Array(wl * hl).fill(Number.MAX_VALUE);
  for (let row = 0; row < h; row += 1) {
    for (let col = 0; col < w; col += 1) {
      const i = (row * w + col) * stride;
      const k = Math.floor(row / s) * wl + Math.floor(col / s);
      const m = Math.min(px[i]! / 255 / air[0]!, px[i + 1]! / 255 / air[1]!, px[i + 2]! / 255 / air[2]!);
      norm[k] = Math.min(norm[k]!, m);
    }
  }
  const spread = new Float32Array(wl * hl);
  for (let row = 0; row < hl; row += 1) {
    for (let col = 0; col < wl; col += 1) {
      let m = Number.MAX_VALUE;
      for (let dy = Math.max(0, row - 1); dy < Math.min(hl, row + 2); dy += 1) {
        for (let dx = Math.max(0, col - 1); dx < Math.min(wl, col + 2); dx += 1) m = Math.min(m, norm[dy * wl + dx]!);
      }
      spread[row * wl + col] = m;
    }
  }
  const omega = 0.95 * amount;
  const tLow = Float32Array.from(spread, (d) => 1 - omega * d);
  const yLow = Float32Array.from({ length: wl * hl }, (_, k) => mean(k, 3));
  const coeffs = guided(yLow, tLow, wl, hl, 2, DEHAZE_EPS, s);
  for (let row = 0; row < h; row += 1) {
    for (let col = 0; col < w; col += 1) {
      const i = (row * w + col) * stride;
      const l = luma(px, i);
      const [a, b] = coeffAt(coeffs, col, row);
      const t = Math.min(Math.max(a * l + b, MIN_TRANSMISSION), 1);
      for (let c = 0; c < 3; c += 1) {
        const v = px[i + c]! / 255;
        px[i + c] = toByte((v - air[c]!) / t + air[c]!);
      }
    }
  }
}

/**
 * ปรับในที่: `px` พิกเซลละ `stride` ช่อง (4 = RGBA ของ canvas — ช่องที่สี่ไม่แตะ)
 * `long` = ด้านยาวของภาพทั้งกรอบเป็นพิกเซลของ `px` (ขยายดูบางส่วน = ด้านยาวของทั้งกรอบ ไม่ใช่ของส่วนที่เห็น)
 */
export function applyLocal(px: Uint8ClampedArray | Uint8Array, width: number, height: number, stride: number, long: number, a: Adjust): void {
  if (width === 0 || height === 0 || !hasLocal(a)) return;
  const [texture, clarity, haze] = [amount(a.texture), amount(a.clarity), amount(a.dehaze)];
  if (haze !== 0) dehaze(px, width, height, stride, long, haze / 100);
  if (texture !== 0 || clarity !== 0) detail(px, width, height, stride, long, texture / 100, clarity / 100);
}
