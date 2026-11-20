/**
 * แสงและสีของการแต่งภาพ — สูตรเดียวกับ `adjust.rs` ใน Rust (เทสต์ค่าเดียวกันทั้งสองฝั่ง)
 * ใช้วาดตัวอย่างสดในหน้าแต่งภาพ ภาพย่อ ตัวดูภาพ และสำเนาที่ส่งออกสร้างด้วย Rust จากค่าชุดเดียวกัน
 */

import type { EditAdjust } from '@/lib/ipc/types';

export type Adjust = EditAdjust;

export const NEUTRAL: Adjust = { exposure: 0, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0, temperature: 0, tint: 0, vibrance: 0, saturation: 0 };

export function isNeutral(a: Adjust): boolean {
  return (Object.keys(NEUTRAL) as (keyof Adjust)[]).every((k) => a[k] === 0);
}

const toLinear = (v: number) => (v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4));
const toSrgb = (v: number) => (v <= 0.0031308 ? v * 12.92 : 1.055 * Math.pow(v, 1 / 2.4) - 0.055);
const clamp01 = (v: number) => Math.min(Math.max(v, 0), 1);

/** ตารางของขั้นที่แยกต่อช่องสีได้: [แดง, เขียว, น้ำเงิน][ค่าเข้า 0–255] → ค่าออก 0–1 */
export function curves(a: Adjust): [Float64Array, Float64Array, Float64Array] {
  const gains = [1 + (0.25 * a.temperature) / 100, 1 - (0.2 * a.tint) / 100, 1 - (0.25 * a.temperature) / 100];
  const light = Math.pow(2, a.exposure);
  const black = (-0.1 * a.blacks) / 100;
  const white = 1 - (0.1 * a.whites) / 100;
  const k = a.contrast / 100;
  const out: [Float64Array, Float64Array, Float64Array] = [new Float64Array(256), new Float64Array(256), new Float64Array(256)];
  out.forEach((table, c) => {
    const gain = gains[c] ?? 1;
    for (let i = 0; i < 256; i += 1) {
      let x = toSrgb(clamp01(toLinear(i / 255) * gain * light));
      x = clamp01((x - black) / (white - black));
      x += ((0.25 * a.shadows) / 100) * 6.75 * x * (1 - x) * (1 - x);
      x += ((0.25 * a.highlights) / 100) * 6.75 * x * x * (1 - x);
      x = clamp01(x);
      x = k >= 0 ? x + k * (x * x * (3 - 2 * x) - x) : 0.5 + (x - 0.5) * (1 + 0.5 * k);
      table[i] = clamp01(x);
    }
  });
  return out;
}

/** ค่าออก 0–255 ของพิกเซลหนึ่ง (เทียบกับ Adjust::pixel ใน Rust) */
export function pixel(a: Adjust, tables: [Float64Array, Float64Array, Float64Array], r: number, g: number, b: number): [number, number, number] {
  let rr = tables[0][r] ?? 0;
  let gg = tables[1][g] ?? 0;
  let bb = tables[2][b] ?? 0;
  if (a.saturation !== 0 || a.vibrance !== 0) {
    const luma = 0.2126 * rr + 0.7152 * gg + 0.0722 * bb;
    const sat = Math.max(rr, gg, bb) - Math.min(rr, gg, bb);
    const factor = (1 + a.saturation / 100) * (1 + (a.vibrance / 100) * (1 - sat));
    rr = clamp01(luma + (rr - luma) * factor);
    gg = clamp01(luma + (gg - luma) * factor);
    bb = clamp01(luma + (bb - luma) * factor);
  }
  return [Math.round(rr * 255), Math.round(gg * 255), Math.round(bb * 255)];
}

/** ปรับพิกเซลของ canvas ทั้งหมด (RGBA) — วนครั้งเดียว ไม่สร้าง array ต่อพิกเซล (ตัวอย่างสดต้องเร็ว) */
export function applyToPixels(data: Uint8ClampedArray, a: Adjust): void {
  if (isNeutral(a)) return;
  const [cr, cg, cb] = curves(a);
  if (a.saturation === 0 && a.vibrance === 0) {
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
