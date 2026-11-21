/**
 * ค่าแสงและสีของการแต่งภาพฝั่ง UI: ค่าเริ่มต้น ชื่อช่วงสี และตัวตรวจว่าส่วนไหนถูกแต่ง
 * — ภาพทุกแบบ (ตัวอย่างสดในหน้าแต่งภาพ ภาพย่อ ตัวดูภาพ สำเนาที่ส่งออก) Rust วาดด้วย `adjust.rs` ชุดเดียว (DECISIONS ข้อ 30)
 */

import type { EditAdjust, Grading, Levels, Vignette, Wheel } from '@/lib/ipc/types';

export type Adjust = EditAdjust;

export const LEVELS_IDENTITY: Levels = { inBlack: 0, inWhite: 255, gamma: 1, outBlack: 0, outWhite: 255 };
const NO_WHEEL: Wheel = { hue: 0, sat: 0, lum: 0 };
export const GRADING_NEUTRAL: Grading = { shadows: NO_WHEEL, midtones: NO_WHEEL, highlights: NO_WHEEL, global: NO_WHEEL, blending: 50, balance: 0 };
export const VIGNETTE_NEUTRAL: Vignette = { amount: 0, midpoint: 50, feather: 50, roundness: 0 };

export const HUE_NAMES = ['แดง', 'ส้ม', 'เหลือง', 'เขียว', 'ฟ้าน้ำทะเล', 'น้ำเงิน', 'ม่วง', 'ม่วงแดง'];
/** สีของจุดหน้าชื่อช่วงสี */
export const HUE_SWATCHES = ['#e5484d', '#f2862f', '#e2bd00', '#3fa34d', '#21a8b8', '#3b6fd8', '#8a4fd1', '#d44fa4'];

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
