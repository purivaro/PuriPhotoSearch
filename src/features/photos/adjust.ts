/**
 * แสงและสีของการแต่งภาพ — สูตรเดียวกับ `adjust.rs` ใน Rust (เทสต์ค่าเดียวกันทั้งสองฝั่ง)
 * ใช้วาดตัวอย่างสดในหน้าแต่งภาพ ภาพย่อ ตัวดูภาพ และสำเนาที่ส่งออกสร้างด้วย Rust จากค่าชุดเดียวกัน
 */

import type { EditAdjust } from '@/lib/ipc/types';

export type Adjust = EditAdjust;

/** แปดช่วงสีแบบ Lightroom (จุดกลางเป็นองศาของ hue) */
const HUE_CENTERS = [0, 30, 60, 120, 180, 240, 270, 300];
export const HUE_NAMES = ['แดง', 'ส้ม', 'เหลือง', 'เขียว', 'ฟ้าน้ำทะเล', 'น้ำเงิน', 'ม่วง', 'ม่วงแดง'];
/** สีของจุดหน้าชื่อช่วงสี */
export const HUE_SWATCHES = ['#e5484d', '#f2862f', '#e2bd00', '#3fa34d', '#21a8b8', '#3b6fd8', '#8a4fd1', '#d44fa4'];
const DEFAULT_SPLITS = [25, 50, 75];

const ZERO8 = [0, 0, 0, 0, 0, 0, 0, 0];

export const NEUTRAL: Adjust = {
  exposure: 0,
  contrast: 0,
  highlights: 0,
  shadows: 0,
  whites: 0,
  blacks: 0,
  temperature: 0,
  tint: 0,
  vibrance: 0,
  saturation: 0,
  hue: ZERO8,
  sat: ZERO8,
  lum: ZERO8,
  parametric: [0, 0, 0, 0],
  parametricSplit: null,
  curve: [],
  curveRed: [],
  curveGreen: [],
  curveBlue: [],
};

const SLIDERS = ['exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks', 'temperature', 'tint', 'vibrance', 'saturation'] as const;
/** ค่าที่เป็นแถบเลื่อนตัวเดียว */
export type SliderKey = (typeof SLIDERS)[number];

const allZero = (v: number[]) => v.every((x) => x === 0);
const curveIsIdentity = (points: [number, number][]) => points.every(([x, y]) => Math.abs(x - y) < 1e-9);
export const hasHsl = (a: Adjust) => !allZero(a.hue) || !allZero(a.sat) || !allZero(a.lum);
export const hasCurves = (a: Adjust) => !allZero(a.parametric) || ![a.curve, a.curveRed, a.curveGreen, a.curveBlue].every(curveIsIdentity);

/** ค่าเท่ากันทุกช่อง (ไม่ขึ้นกับลำดับ key ของ object ที่มาจาก Rust หรือ UI) */
export function sameAdjust(a: Adjust, b: Adjust): boolean {
  return (Object.keys(NEUTRAL) as (keyof Adjust)[]).every((k) => JSON.stringify(a[k]) === JSON.stringify(b[k]));
}

export function isNeutral(a: Adjust): boolean {
  return SLIDERS.every((k) => a[k] === 0) && !hasHsl(a) && !hasCurves(a);
}

const toLinear = (v: number) => (v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4));
const toSrgb = (v: number) => (v <= 0.0031308 ? v * 12.92 : 1.055 * Math.pow(v, 1 / 2.4) - 0.055);
const clamp01 = (v: number) => Math.min(Math.max(v, 0), 1);

/** Parametric curve: สี่ช่วงตามจุดแบ่ง ยก/กดแต่ละช่วงด้วยน้ำหนักรูประฆัง ปลายสองข้างไม่ขยับ */
function parametric(x: number, amounts: number[], splits: number[]): number {
  if (allZero(amounts)) return x;
  const edges = [0, splits[0]!, splits[1]!, splits[2]!, 1];
  const pin = Math.min(8 * x * (1 - x), 1);
  let out = x;
  amounts.forEach((amount, i) => {
    const a = edges[i]!;
    const b = edges[i + 1]!;
    const center = (a + b) / 2;
    const width = Math.max((b - a) * 0.75, 0.02);
    const weight = Math.exp(-(((x - center) / width) ** 2));
    out += (amount / 100) * 0.12 * weight * pin;
  });
  return clamp01(out);
}

/** เส้นโค้งผ่านจุดแบบ monotone cubic (Fritsch–Carlson): ไม่แกว่งเกินจุด — ค่านอกช่วงจุดแรก/สุดท้ายเท่าจุดปลาย */
function spline(points: [number, number][]): (x: number) => number {
  if (points.length < 2 || curveIsIdentity(points)) return (x) => x;
  const xs = points.map((p) => p[0] / 255);
  const ys = points.map((p) => p[1] / 255);
  const n = xs.length;
  const d = Array.from({ length: n - 1 }, (_, k) => (ys[k + 1]! - ys[k]!) / (xs[k + 1]! - xs[k]!));
  const ms = new Array<number>(n).fill(0);
  ms[0] = d[0]!;
  ms[n - 1] = d[n - 2]!;
  for (let k = 1; k < n - 1; k += 1) ms[k] = d[k - 1]! * d[k]! <= 0 ? 0 : (d[k - 1]! + d[k]!) / 2;
  for (let k = 0; k < n - 1; k += 1) {
    const dk = d[k]!;
    if (dk === 0) {
      ms[k] = 0;
      ms[k + 1] = 0;
      continue;
    }
    const a = ms[k]! / dk;
    const b = ms[k + 1]! / dk;
    const len = a * a + b * b;
    if (len > 9) {
      const tau = 3 / Math.sqrt(len);
      ms[k] = tau * a * dk;
      ms[k + 1] = tau * b * dk;
    }
  }
  return (x) => {
    if (x <= xs[0]!) return ys[0]!;
    if (x >= xs[n - 1]!) return ys[n - 1]!;
    let k = 0;
    while (k < n - 2 && x >= xs[k + 1]!) k += 1;
    const h = xs[k + 1]! - xs[k]!;
    const t = (x - xs[k]!) / h;
    const t2 = t * t;
    const t3 = t2 * t;
    return (2 * t3 - 3 * t2 + 1) * ys[k]! + (t3 - 2 * t2 + t) * h * ms[k]! + (-2 * t3 + 3 * t2) * ys[k + 1]! + (t3 - t2) * h * ms[k + 1]!;
  };
}

/** ตารางของขั้นที่แยกต่อช่องสีได้: [แดง, เขียว, น้ำเงิน][ค่าเข้า 0–255] → ค่าออก 0–1 */
export function curves(a: Adjust): [Float64Array, Float64Array, Float64Array] {
  const gains = [1 + (0.25 * a.temperature) / 100, 1 - (0.2 * a.tint) / 100, 1 - (0.25 * a.temperature) / 100];
  const light = Math.pow(2, a.exposure);
  const black = (-0.1 * a.blacks) / 100;
  const white = 1 - (0.1 * a.whites) / 100;
  const k = a.contrast / 100;
  const master = spline(a.curve);
  const channel = [spline(a.curveRed), spline(a.curveGreen), spline(a.curveBlue)];
  const splits = (a.parametricSplit ?? DEFAULT_SPLITS).map((v) => v / 100);
  const out: [Float64Array, Float64Array, Float64Array] = [new Float64Array(256), new Float64Array(256), new Float64Array(256)];
  out.forEach((table, c) => {
    const gain = gains[c] ?? 1;
    const own = channel[c]!;
    for (let i = 0; i < 256; i += 1) {
      let x = toSrgb(clamp01(toLinear(i / 255) * gain * light));
      x = clamp01((x - black) / (white - black));
      x += ((0.25 * a.shadows) / 100) * 6.75 * x * (1 - x) * (1 - x);
      x += ((0.25 * a.highlights) / 100) * 6.75 * x * x * (1 - x);
      x = clamp01(x);
      x = k >= 0 ? x + k * (x * x * (3 - 2 * x) - x) : 0.5 + (x - 0.5) * (1 + 0.5 * k);
      x = parametric(clamp01(x), a.parametric, splits);
      x = own(master(x));
      table[i] = clamp01(x);
    }
  });
  return out;
}

/** ช่วงสีที่ hue อยู่ระหว่าง (ช่วงซ้าย) — ระยะไปช่วงขวาคำนวณต่อจากนี้ วนรอบ 360° */
function hueSegment(h: number): number {
  for (let k = 7; k > 0; k -= 1) if (h >= HUE_CENTERS[k]!) return k;
  return 0;
}

/**
 * ขั้นต่อพิกเซล (ค่า 0–1 หลังผ่านตาราง) เขียนผลลง `out`: HSL ต่อช่วงสี แล้วความสด/ความอิ่มสี
 * ไม่สร้าง array ต่อพิกเซล — ตัวอย่างสดทั้งภาพต้องเร็วพอให้ลากแถบได้ลื่น
 */
function colorInto(a: Adjust, withHsl: boolean, r: number, g: number, b: number, out: Float64Array): void {
  let rr = r;
  let gg = g;
  let bb = b;
  if (withHsl) {
    // RGB → HSL
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const l = (max + min) / 2;
    const d = max - min;
    const s = d <= 1e-12 ? 0 : l > 0.5 ? d / (2 - max - min) : d / (max + min);
    if (s > 1e-6) {
      let h = max === r ? ((((g - b) / d) % 6) + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
      h *= 60;
      // น้ำหนักของสองช่วงสีที่ hue อยู่ระหว่างกลาง · ผลแรงตามความอิ่มของพิกเซล (สีเทาไม่เปลี่ยน)
      const k = hueSegment(h);
      const k1 = (k + 1) % 8;
      const from = HUE_CENTERS[k]!;
      const t = (h - from) / ((k === 7 ? 360 : HUE_CENTERS[k + 1]!) - from);
      const strength = Math.min(s * 1.5, 1);
      const h2 = (((h + ((a.hue[k]! * (1 - t) + a.hue[k1]! * t) / 100) * 30 * strength) % 360) + 360) % 360;
      const s2 = clamp01(s * (1 + ((a.sat[k]! * (1 - t) + a.sat[k1]! * t) / 100) * strength));
      const l2 = clamp01(l + ((a.lum[k]! * (1 - t) + a.lum[k1]! * t) / 100) * 0.25 * strength);
      // HSL → RGB
      const c = (1 - Math.abs(2 * l2 - 1)) * s2;
      const hp = h2 / 60;
      const x = c * (1 - Math.abs((hp % 2) - 1));
      const sector = Math.floor(hp);
      const m = l2 - c / 2;
      rr = sector === 0 || sector === 5 ? c : sector === 1 || sector === 4 ? x : 0;
      gg = sector === 1 || sector === 2 ? c : sector === 0 || sector === 3 ? x : 0;
      bb = sector === 3 || sector === 4 ? c : sector === 2 || sector === 5 ? x : 0;
      rr = clamp01(rr + m);
      gg = clamp01(gg + m);
      bb = clamp01(bb + m);
    }
  }
  if (a.saturation !== 0 || a.vibrance !== 0) {
    const luma = 0.2126 * rr + 0.7152 * gg + 0.0722 * bb;
    const spread = Math.max(rr, gg, bb) - Math.min(rr, gg, bb);
    const factor = (1 + a.saturation / 100) * (1 + (a.vibrance / 100) * (1 - spread));
    rr = clamp01(luma + (rr - luma) * factor);
    gg = clamp01(luma + (gg - luma) * factor);
    bb = clamp01(luma + (bb - luma) * factor);
  }
  out[0] = rr;
  out[1] = gg;
  out[2] = bb;
}

/** ค่าออก 0–255 ของพิกเซลหนึ่ง (เทียบกับ Adjust::pixel ใน Rust) */
export function pixel(a: Adjust, tables: [Float64Array, Float64Array, Float64Array], r: number, g: number, b: number): [number, number, number] {
  const out = new Float64Array(3);
  colorInto(a, hasHsl(a), tables[0][r] ?? 0, tables[1][g] ?? 0, tables[2][b] ?? 0, out);
  return [Math.round(out[0]! * 255), Math.round(out[1]! * 255), Math.round(out[2]! * 255)];
}

/** ปรับพิกเซลของ canvas ทั้งหมด (RGBA) — ตัวอย่างสดต้องเร็ว: ใช้ตารางตรง ๆ เมื่อไม่มีขั้นต่อพิกเซล */
export function applyToPixels(data: Uint8ClampedArray, a: Adjust): void {
  if (isNeutral(a)) return;
  const [cr, cg, cb] = curves(a);
  const withHsl = hasHsl(a);
  if (a.saturation === 0 && a.vibrance === 0 && !withHsl) {
    const tr = Uint8Array.from(cr, (v) => Math.round(v * 255));
    const tg = Uint8Array.from(cg, (v) => Math.round(v * 255));
    const tb = Uint8Array.from(cb, (v) => Math.round(v * 255));
    for (let i = 0; i < data.length; i += 4) {
      data[i] = tr[data[i]!]!;
      data[i + 1] = tg[data[i + 1]!]!;
      data[i + 2] = tb[data[i + 2]!]!;
    }
    return;
  }
  if (!withHsl) {
    // ความสด/ความอิ่มสีอย่างเดียว: วนครั้งเดียว ไม่สร้าง array ต่อพิกเซล
    const sat = 1 + a.saturation / 100;
    const vib = a.vibrance / 100;
    for (let i = 0; i < data.length; i += 4) {
      const r = cr[data[i]!]!;
      const g = cg[data[i + 1]!]!;
      const b = cb[data[i + 2]!]!;
      const luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
      const spread = (r > g ? (r > b ? r : b) : g > b ? g : b) - (r < g ? (r < b ? r : b) : g < b ? g : b);
      const factor = sat * (1 + vib * (1 - spread));
      // ปัดแบบเดียวกับ Rust แล้วให้ Uint8ClampedArray หนีบ 0–255
      data[i] = Math.round((luma + (r - luma) * factor) * 255);
      data[i + 1] = Math.round((luma + (g - luma) * factor) * 255);
      data[i + 2] = Math.round((luma + (b - luma) * factor) * 255);
    }
    return;
  }
  const out = new Float64Array(3);
  for (let i = 0; i < data.length; i += 4) {
    colorInto(a, true, cr[data[i]!]!, cg[data[i + 1]!]!, cb[data[i + 2]!]!, out);
    data[i] = Math.round(out[0]! * 255);
    data[i + 1] = Math.round(out[1]! * 255);
    data[i + 2] = Math.round(out[2]! * 255);
  }
}

/**
 * ปรับอัตโนมัติจากภาพตัวอย่าง (ระมัดระวังแบบ "ช่วยให้ดีขึ้น" ไม่ใช่เปลี่ยนอารมณ์ภาพ):
 * ค่าแสง — แตะเฉพาะภาพที่ค่ากลางความสว่างมืดกว่า 30% หรือสว่างกว่า 62% และไม่เกิน ±1 EV
 * จุดดำ/จุดขาว — ยืดช่วงให้เต็ม (0.5% และ 99.5% หลังปรับค่าแสง) ไม่เกิน ±60 · ค่าสีอื่นไม่แตะ
 */
export function autoAdjust(data: Uint8ClampedArray): Adjust {
  const hist = new Float64Array(256);
  let count = 0;
  for (let i = 0; i < data.length; i += 16) {
    const luma = 0.2126 * data[i]! + 0.7152 * data[i + 1]! + 0.0722 * data[i + 2]!;
    hist[Math.round(luma)]! += 1;
    count += 1;
  }
  if (count === 0) return NEUTRAL;
  const percentile = (h: Float64Array, p: number) => {
    let seen = 0;
    for (let v = 0; v < 256; v += 1) {
      seen += h[v]!;
      if (seen >= count * p) return v / 255;
    }
    return 1;
  };
  const round = (v: number, step: number) => Math.round(v / step) * step;
  const median = Math.min(Math.max(percentile(hist, 0.5), 0.02), 0.98);
  const target = median < 0.3 ? 0.36 : median > 0.62 ? 0.56 : median;
  const exposure = Math.min(Math.max(round(Math.log2(toLinear(target) / toLinear(median)), 0.05), -1), 1);
  // ฮิสโตแกรมหลังปรับค่าแสง แล้วจึงหาจุดดำ/จุดขาว
  const [table] = curves({ ...NEUTRAL, exposure });
  const shifted = new Float64Array(256);
  hist.forEach((n, v) => {
    shifted[Math.round(table[v]! * 255)]! += n;
  });
  const low = percentile(shifted, 0.005);
  const high = percentile(shifted, 0.995);
  return {
    ...NEUTRAL,
    exposure,
    blacks: Math.max(-60, round(-low * 1000, 1)),
    whites: Math.min(60, round((1 - high) * 1000, 1)),
  };
}
