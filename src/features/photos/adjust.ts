/**
 * แสงและสีของการแต่งภาพ — สูตรเดียวกับ `adjust.rs` ใน Rust (เทสต์ค่าเดียวกันทั้งสองฝั่ง)
 * ใช้วาดตัวอย่างสดในหน้าแต่งภาพ ภาพย่อ ตัวดูภาพ และสำเนาที่ส่งออกสร้างด้วย Rust จากค่าชุดเดียวกัน
 */

import type { EditAdjust, Grading, Levels, Vignette, Wheel } from '@/lib/ipc/types';

export type Adjust = EditAdjust;

export const LEVELS_IDENTITY: Levels = { inBlack: 0, inWhite: 255, gamma: 1, outBlack: 0, outWhite: 255 };
const NO_WHEEL: Wheel = { hue: 0, sat: 0, lum: 0 };
export const GRADING_NEUTRAL: Grading = { shadows: NO_WHEEL, midtones: NO_WHEEL, highlights: NO_WHEEL, global: NO_WHEEL, blending: 50, balance: 0 };
export const VIGNETTE_NEUTRAL: Vignette = { amount: 0, midpoint: 50, feather: 50, roundness: 0 };

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
  texture: 0,
  clarity: 0,
  dehaze: 0,
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
  levels: LEVELS_IDENTITY,
  levelsRed: LEVELS_IDENTITY,
  levelsGreen: LEVELS_IDENTITY,
  levelsBlue: LEVELS_IDENTITY,
  grading: GRADING_NEUTRAL,
  vignette: VIGNETTE_NEUTRAL,
};

const SLIDERS = ['exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks', 'texture', 'clarity', 'dehaze', 'temperature', 'tint', 'vibrance', 'saturation'] as const;
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

export const levelsIsIdentity = (l: Levels) =>
  l.inBlack === 0 && l.inWhite === 255 && l.gamma === 1 && l.outBlack === 0 && l.outWhite === 255;
export const hasLevels = (a: Adjust) => ![a.levels, a.levelsRed, a.levelsGreen, a.levelsBlue].every(levelsIsIdentity);
export const gradingIsActive = (g: Grading) => [g.shadows, g.midtones, g.highlights, g.global].some((w) => w.sat !== 0 || w.lum !== 0);
export const hasVignette = (a: Adjust) => a.vignette.amount !== 0;

export function isNeutral(a: Adjust): boolean {
  // ช่องที่ไม่มีในค่าที่บันทึกก่อนรุ่นนี้ (เช่น texture) = 0
  return SLIDERS.every((k) => (a[k] ?? 0) === 0) && !hasHsl(a) && !hasCurves(a) && !hasLevels(a) && !gradingIsActive(a.grading) && !hasVignette(a);
}

const toLinear = (v: number) => (v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4));
const toSrgb = (v: number) => (v <= 0.0031308 ? v * 12.92 : 1.055 * Math.pow(v, 1 / 2.4) - 0.055);
const clamp01 = (v: number) => Math.min(Math.max(v, 0), 1);
const smooth = (t: number) => {
  const c = clamp01(t);
  return c * c * (3 - 2 * c);
};
const luma = (r: number, g: number, b: number) => 0.2126 * r + 0.7152 * g + 0.0722 * b;

/** Levels: ค่าเข้า 0–1 → ค่าออก 0–1 */
function levels(l: Levels, x: number): number {
  if (levelsIsIdentity(l)) return x;
  const v = clamp01((x * 255 - l.inBlack) / (l.inWhite - l.inBlack));
  return (l.outBlack + Math.pow(v, 1 / l.gamma) * (l.outWhite - l.outBlack)) / 255;
}

/** เศษเหลือแบบไม่ติดลบ เท่ากับ f64::rem_euclid ของ Rust ทุกบิต (`((x % m) + m) % m` ปัดเศษเพิ่มอีกครั้ง ผลต่างกันที่บิตท้าย) */
const remEuclid = (x: number, m: number) => {
  const r = x % m;
  return r < 0 ? r + Math.abs(m) : r;
};

/** สีเต็มของ hue (HSL ความอิ่ม 1 ความสว่าง 0.5) */
function pureHue(h: number): [number, number, number] {
  const hp = h / 60;
  const x = 1 - Math.abs((hp % 2) - 1);
  // 360° (= ช่วงที่ 6) ใช้ช่วงสุดท้ายเหมือน `_ =>` ของ hsl_to_rgb ใน Rust
  const sector = Math.min(Math.floor(hp), 5);
  const r = sector === 0 || sector === 5 ? 1 : sector === 1 || sector === 4 ? x : 0;
  const g = sector === 1 || sector === 2 ? 1 : sector === 0 || sector === 3 ? x : 0;
  const b = sector === 3 || sector === 4 ? 1 : sector === 2 || sector === 5 ? x : 0;
  return [r, g, b];
}

/** Color Grading: เติมสีตามน้ำหนักของช่วงความสว่างที่พิกเซลอยู่ (ค่าในและออก 0–1) — เขียนผลลง `out` */
function gradeInto(g: Grading, r: number, gg: number, b: number, out: Float64Array): void {
  const lp = clamp01(luma(r, gg, b) + (g.balance / 100) * 0.25);
  const width = 0.25 + (0.5 * g.blending) / 100;
  const weights = [smooth(1 - lp / width), smooth(1 - Math.abs(lp - 0.5) / width), smooth(1 - (1 - lp) / width), 1];
  const wheels = [g.shadows, g.midtones, g.highlights, g.global];
  let o0 = r;
  let o1 = gg;
  let o2 = b;
  for (let k = 0; k < 4; k += 1) {
    const w = weights[k]!;
    const wheel = wheels[k]!;
    if (w <= 0) continue;
    if (wheel.sat > 0) {
      const [cr, cg, cb] = pureHue(remEuclid(wheel.hue, 360));
      const cl = luma(cr, cg, cb);
      const f = (wheel.sat / 100) * 0.35 * w;
      o0 += (cr - cl) * f;
      o1 += (cg - cl) * f;
      o2 += (cb - cl) * f;
    }
    if (wheel.lum !== 0) {
      const d = (wheel.lum / 100) * 0.2 * w;
      o0 += d;
      o1 += d;
      o2 += d;
    }
  }
  out[0] = clamp01(o0);
  out[1] = clamp01(o1);
  out[2] = clamp01(o2);
}

/** น้ำหนัก Vignette 0–1 ของพิกเซลที่ (x, y) — จุดกลางของพิกเซล ในภาพขนาด w × h (เทียบ Vignette::weight ใน Rust) */
export function vignetteWeight(v: Vignette, x: number, y: number, w: number, h: number): number {
  const u = (x / w) * 2 - 1;
  const vv = (y / h) * 2 - 1;
  const m = Math.max(w, h);
  const ellipse = Math.sqrt(u * u + vv * vv) / Math.SQRT2;
  const r = v.roundness / 100;
  const d =
    r >= 0
      ? ellipse + (Math.sqrt(((u * w) / m) ** 2 + ((vv * h) / m) ** 2) / Math.SQRT2 - ellipse) * r
      : ellipse + (Math.max(Math.abs(u), Math.abs(vv)) - ellipse) * -r;
  const start = 0.15 + (v.midpoint / 100) * 0.75;
  const width = 0.05 + (v.feather / 100) * 0.95;
  return smooth((d - start) / width);
}

function vignetteInto(v: Vignette, t: number, out: Float64Array): void {
  const a = (v.amount / 100) * t;
  for (let i = 0; i < 3; i += 1) out[i] = a < 0 ? out[i]! * (1 + a) : out[i]! + (1 - out[i]!) * a;
}

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
  return splineXY(
    points.map((p) => p[0] / 255),
    points.map((p) => p[1] / 255),
  );
}

/** เหมือน `spline` แต่รับจุดใดก็ได้ (x เพิ่มขึ้น) อย่างน้อยสองจุด — สูตรเดียวกับ `Spline::from_xy` ใน Rust */
function splineXY(xs: number[], ys: number[]): (x: number) => number {
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

/** ฟังก์ชันของเส้นโค้งแบบจุด (ค่าเข้า/ออก 0–1) — ใช้วาดกราฟในหน้าแต่งภาพ */
export function curveFunction(points: [number, number][]): (x: number) => number {
  return spline(points);
}

/** ฮิสโตแกรมของภาพที่แสดง: จำนวนพิกเซลต่อค่า 0–255 ของแต่ละช่อง (สุ่มนับทุก 4 พิกเซล) */
export interface Histogram {
  r: Uint32Array;
  g: Uint32Array;
  b: Uint32Array;
}

export function histogram(data: Uint8ClampedArray): Histogram {
  const h: Histogram = { r: new Uint32Array(256), g: new Uint32Array(256), b: new Uint32Array(256) };
  for (let i = 0; i < data.length; i += 16) {
    h.r[data[i]!]! += 1;
    h.g[data[i + 1]!]! += 1;
    h.b[data[i + 2]!]! += 1;
  }
  return h;
}

/**
 * สมดุลแสงขาวจากจุดที่ควรเป็นสีเทา (หลอดดูดสี): อุณหภูมิสี/โทนสีที่ทำให้สีของจุดนั้นเป็นกลาง
 * แก้สมการของ `curves` (คูณในแสงเชิงเส้น): แดง × (1 + 0.25T) = น้ำเงิน × (1 − 0.25T) = เขียว × (1 − 0.2Ti)
 */
export function neutralize(r: number, g: number, b: number, profile: Profile = 'display'): { temperature: number; tint: number } {
  const decode = profile === 'raw' ? rawToneInverse : toLinear;
  const [rl, gl, bl] = [r, g, b].map((v) => Math.max(decode(v / 255), 1e-4)) as [number, number, number];
  const t = Math.min(Math.max((bl - rl) / (0.25 * (rl + bl)), -1), 1);
  const target = rl * (1 + 0.25 * t);
  const ti = Math.min(Math.max((1 - target / gl) / 0.2, -1), 1);
  return { temperature: Math.round(t * 100), tint: Math.round(ti * 100) };
}

/**
 * ชนิดของภาพต้นทาง: ภาพทั่วไป (sRGB) หรือ RAW — ภาพที่หน้าแต่งภาพใช้ของรูป RAW คือโทนตั้งต้น 8 บิต ซึ่งถอดกลับเป็นแสงเชิงเส้น
 * ด้วย `rawToneInverse` แล้วจึงปรับค่าแสง/สมดุลแสงขาวก่อนทำโทนซ้ำ (DECISIONS ข้อ 25) — สูตรเดียวกับ `Profile` ใน Rust
 */
export type Profile = 'display' | 'raw';

/** เส้นโค้งโทนตั้งต้นของ RAW: (log2 ของแสงเชิงเส้นเทียบระดับขาวของเซนเซอร์, ค่าที่แสดง) — ค่าเดียวกับ `RAW_TONE` ใน adjust.rs */
const RAW_TONE: [number, number][] = [
  [-10, 0.012],
  [-9, 0.018],
  [-8, 0.028],
  [-7, 0.054],
  [-6, 0.107],
  [-5, 0.21],
  [-4, 0.356],
  [-3, 0.558],
  [-2.5, 0.66],
  [-2, 0.756],
  [-1.5, 0.842],
  [-1, 0.909],
  [-0.5, 0.958],
  [0, 0.993],
  [1, 1],
];
/** ค่าเชิงเส้นสูงสุดที่เก็บไว้เหนือระดับขาวของเซนเซอร์ (เท่า `raw::HEADROOM`) */
const RAW_HEADROOM = 2;
let rawSpline: ((x: number) => number) | null = null;

/** โทนตั้งต้นของ RAW: แสงเชิงเส้น (1 = ระดับขาวของเซนเซอร์) → ค่าที่แสดง 0–1 */
export function rawTone(x: number): number {
  if (!(x > 0)) return 0;
  const l = Math.log2(x);
  const [first] = RAW_TONE[0]!;
  if (l <= first) return (RAW_TONE[0]![1] * x) / Math.pow(2, first);
  if (l >= RAW_TONE[RAW_TONE.length - 1]![0]) return 1;
  rawSpline ??= splineXY(
    RAW_TONE.map((p) => p[0]),
    RAW_TONE.map((p) => p[1]),
  );
  return rawSpline(l);
}

/** ย้อน `rawTone` (ค้นแบบแบ่งครึ่ง) */
export function rawToneInverse(v: number): number {
  if (v <= 0) return 0;
  if (v >= 1) return RAW_HEADROOM;
  let lo = 0;
  let hi = RAW_HEADROOM;
  for (let i = 0; i < 60; i += 1) {
    const mid = (lo + hi) / 2;
    if (rawTone(mid) < v) lo = mid;
    else hi = mid;
  }
  return (lo + hi) / 2;
}

let rawDecoded: Float64Array | null = null;

/** ค่าเข้า 8 บิต → แสงเชิงเส้น ตามชนิดของภาพต้นทาง */
function decodeTable(profile: Profile): Float64Array {
  if (profile === 'raw') {
    rawDecoded ??= Float64Array.from({ length: 256 }, (_, i) => rawToneInverse(i / 255));
    return rawDecoded;
  }
  return Float64Array.from({ length: 256 }, (_, i) => toLinear(i / 255));
}

/** ตารางของขั้นที่แยกต่อช่องสีได้: [แดง, เขียว, น้ำเงิน][ค่าเข้า 0–255] → ค่าออก 0–1 */
export function curves(a: Adjust, profile: Profile = 'display'): [Float64Array, Float64Array, Float64Array] {
  const decoded = decodeTable(profile);
  const gains = [1 + (0.25 * a.temperature) / 100, 1 - (0.2 * a.tint) / 100, 1 - (0.25 * a.temperature) / 100];
  const light = Math.pow(2, a.exposure);
  const black = (-0.1 * a.blacks) / 100;
  const white = 1 - (0.1 * a.whites) / 100;
  const k = a.contrast / 100;
  const master = spline(a.curve);
  const channel = [spline(a.curveRed), spline(a.curveGreen), spline(a.curveBlue)];
  const channelLevels = [a.levelsRed, a.levelsGreen, a.levelsBlue];
  const splits = (a.parametricSplit ?? DEFAULT_SPLITS).map((v) => v / 100);
  const out: [Float64Array, Float64Array, Float64Array] = [new Float64Array(256), new Float64Array(256), new Float64Array(256)];
  out.forEach((table, c) => {
    const gain = gains[c] ?? 1;
    const own = channel[c]!;
    for (let i = 0; i < 256; i += 1) {
      const linear = decoded[i]! * gain * light;
      let x = profile === 'raw' ? rawTone(linear) : toSrgb(clamp01(linear));
      x = clamp01((x - black) / (white - black));
      x += ((0.25 * a.shadows) / 100) * 6.75 * x * (1 - x) * (1 - x);
      x += ((0.25 * a.highlights) / 100) * 6.75 * x * x * (1 - x);
      x = clamp01(x);
      x = k >= 0 ? x + k * (x * x * (3 - 2 * x) - x) : 0.5 + (x - 0.5) * (1 + 0.5 * k);
      x = parametric(clamp01(x), a.parametric, splits);
      x = own(levels(channelLevels[c]!, master(levels(a.levels, x))));
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
      let h = max === r ? remEuclid((g - b) / d, 6) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
      h *= 60;
      // น้ำหนักของสองช่วงสีที่ hue อยู่ระหว่างกลาง · ผลแรงตามความอิ่มของพิกเซล (สีเทาไม่เปลี่ยน)
      const k = hueSegment(h);
      const k1 = (k + 1) % 8;
      const from = HUE_CENTERS[k]!;
      const t = (h - from) / ((k === 7 ? 360 : HUE_CENTERS[k + 1]!) - from);
      const strength = Math.min(s * 1.5, 1);
      const h2 = remEuclid(h + ((a.hue[k]! * (1 - t) + a.hue[k1]! * t) / 100) * 30 * strength, 360);
      const s2 = clamp01(s * (1 + ((a.sat[k]! * (1 - t) + a.sat[k1]! * t) / 100) * strength));
      const l2 = clamp01(l + ((a.lum[k]! * (1 - t) + a.lum[k1]! * t) / 100) * 0.25 * strength);
      // HSL → RGB
      const c = (1 - Math.abs(2 * l2 - 1)) * s2;
      const hp = h2 / 60;
      const x = c * (1 - Math.abs((hp % 2) - 1));
      // h2 = 360° พอดี (= ช่วงที่ 6) ใช้ช่วงสุดท้ายเหมือน `_ =>` ของ hsl_to_rgb ใน Rust
      const sector = Math.min(Math.floor(hp), 5);
      const m = l2 - c / 2;
      rr = sector === 0 || sector === 5 ? c : sector === 1 || sector === 4 ? x : 0;
      gg = sector === 1 || sector === 2 ? c : sector === 0 || sector === 3 ? x : 0;
      bb = sector === 3 || sector === 4 ? c : sector === 2 || sector === 5 ? x : 0;
      rr = clamp01(rr + m);
      gg = clamp01(gg + m);
      bb = clamp01(bb + m);
    }
  }
  if (gradingIsActive(a.grading)) {
    gradeInto(a.grading, rr, gg, bb, scratch);
    rr = scratch[0]!;
    gg = scratch[1]!;
    bb = scratch[2]!;
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

const scratch = new Float64Array(3);

/** ค่าออก 0–255 ของพิกเซลหนึ่ง (เทียบกับ Adjust::pixel ใน Rust) */
export function pixel(a: Adjust, tables: [Float64Array, Float64Array, Float64Array], r: number, g: number, b: number): [number, number, number] {
  return pixelAt(a, tables, r, g, b, 0);
}

/** เหมือน `pixel` พร้อมน้ำหนัก Vignette ของตำแหน่งพิกเซล (เทียบกับ Adjust::pixel_at ใน Rust) */
export function pixelAt(a: Adjust, tables: [Float64Array, Float64Array, Float64Array], r: number, g: number, b: number, vignette: number): [number, number, number] {
  const out = new Float64Array(3);
  colorInto(a, hasHsl(a), tables[0][r] ?? 0, tables[1][g] ?? 0, tables[2][b] ?? 0, out);
  if (vignette > 0) vignetteInto(a.vignette, vignette, out);
  return [Math.round(clamp01(out[0]!) * 255), Math.round(clamp01(out[1]!) * 255), Math.round(clamp01(out[2]!) * 255)];
}

/**
 * ปรับพิกเซลของ canvas ทั้งหมด (RGBA) — ตัวอย่างสดต้องเร็ว: ใช้ตารางตรง ๆ เมื่อไม่มีขั้นต่อพิกเซล
 * `width` = ความกว้างของภาพ เมื่อภาพนี้คือกรอบหลังตัดทั้งกรอบ (ให้คิด Vignette ได้) · ไม่ส่ง = ไม่ใส่ Vignette
 */
export function applyToPixels(data: Uint8ClampedArray, a: Adjust, width?: number, profile: Profile = 'display'): void {
  // ไม่ได้แต่ง = ภาพเดิม ทั้งภาพทั่วไปและภาพโทนตั้งต้นของ RAW
  if (isNeutral(a)) return;
  const [cr, cg, cb] = curves(a, profile);
  const withHsl = hasHsl(a);
  const withVignette = hasVignette(a) && width !== undefined && width > 0;
  const perPixel = withHsl || gradingIsActive(a.grading);
  if (a.saturation === 0 && a.vibrance === 0 && !perPixel && !withVignette) {
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
  if (!perPixel && !withVignette) {
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
  const w = width ?? 1;
  const h = data.length / 4 / w;
  for (let i = 0; i < data.length; i += 4) {
    colorInto(a, withHsl, cr[data[i]!]!, cg[data[i + 1]!]!, cb[data[i + 2]!]!, out);
    if (withVignette) {
      const p = i / 4;
      const t = vignetteWeight(a.vignette, (p % w) + 0.5, Math.floor(p / w) + 0.5, w, h);
      if (t > 0) vignetteInto(a.vignette, t, out);
    }
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
export function autoAdjust(data: Uint8ClampedArray, profile: Profile = 'display'): Adjust {
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
  // ค่าแสงทำในแสงเชิงเส้น: RAW ถอดกลับด้วยเส้นโค้งโทนตั้งต้น
  const decode = profile === 'raw' ? rawToneInverse : toLinear;
  const exposure = Math.min(Math.max(round(Math.log2(decode(target) / decode(median)), 0.05), -1), 1);
  // ฮิสโตแกรมหลังปรับค่าแสง แล้วจึงหาจุดดำ/จุดขาว
  const [table] = curves({ ...NEUTRAL, exposure }, profile);
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
