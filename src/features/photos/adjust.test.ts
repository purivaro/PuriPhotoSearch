import { describe, expect, it } from 'vitest';
import { NEUTRAL, applyToPixels, autoAdjust, curves, isNeutral, pixel, pixelAt, vignetteWeight, type Adjust } from './adjust';

// ค่าเดียวกับ adjust::tests::sample ใน Rust
const sample: Adjust = { ...NEUTRAL, exposure: 0.5, contrast: 30, highlights: -40, shadows: 50, whites: 10, blacks: -10, temperature: 20, tint: -10, vibrance: 25, saturation: 10 };
// ค่าเดียวกับ adjust::tests::sample_full (แบบ preset "more white skin": HSL + เส้นโค้ง + parametric)
const sampleFull: Adjust = {
  ...sample,
  hue: [0, 3, -1, 7, 0, 5, 0, 0],
  sat: [32, -3, -13, -33, 15, -9, 0, 0],
  lum: [20, 42, 2, 11, 25, 0, 5, 6],
  parametric: [-5, 4, 12, 0],
  curve: [
    [2, 11],
    [60, 60],
    [138, 145],
    [255, 255],
  ],
};

// ค่าเดียวกับ adjust::tests::sample_pro (sample_full + Levels + Color Grading + Vignette)
const samplePro: Adjust = {
  ...sampleFull,
  levels: { inBlack: 10, inWhite: 240, gamma: 1.3, outBlack: 5, outWhite: 250 },
  levelsBlue: { inBlack: 0, inWhite: 230, gamma: 0.9, outBlack: 0, outWhite: 255 },
  grading: {
    shadows: { hue: 220, sat: 30, lum: -10 },
    midtones: { hue: 35, sat: 15, lum: 0 },
    highlights: { hue: 45, sat: 25, lum: 5 },
    global: { hue: 0, sat: 0, lum: 0 },
    blending: 60,
    balance: -20,
  },
  vignette: { amount: -40, midpoint: 40, feather: 60, roundness: 20 },
};

describe('แสงและสี (สูตรเดียวกับ Rust)', () => {
  it('ได้ค่าเดียวกับ adjust::tests::matches_the_ui_formula ทุกพิกเซล', () => {
    const t = curves(sample);
    expect(pixel(sample, t, 10, 20, 30)).toEqual([11, 30, 47]);
    expect(pixel(sample, t, 120, 90, 200)).toEqual([148, 108, 251]);
    expect(pixel(sample, t, 240, 230, 220)).toEqual([255, 255, 252]);
    const full = curves(sampleFull);
    expect([pixel(sampleFull, full, 10, 20, 30), pixel(sampleFull, full, 200, 130, 80), pixel(sampleFull, full, 60, 140, 90)]).toEqual([
      [20, 37, 53],
      [255, 190, 135],
      [77, 179, 116],
    ]);
  });

  it('Levels + Color Grading + Vignette ได้ค่าเดียวกับ Rust (PRO_EXPECTED)', () => {
    const t = curves(samplePro);
    const at = (rgb: [number, number, number], x: number, y: number) => pixelAt(samplePro, t, ...rgb, vignetteWeight(samplePro.vignette, x, y, 400, 300));
    expect(at([10, 20, 30], 200.5, 150.5)).toEqual([9, 46, 82]);
    expect(at([200, 130, 80], 20.5, 15.5)).toEqual([177, 147, 107]);
    expect(at([60, 140, 90], 390.5, 150.5)).toEqual([99, 176, 121]);
  });

  it('วนทั้งภาพพร้อม Vignette ได้ค่าเดียวกับสูตรต่อพิกเซล', () => {
    const w = 4;
    const h = 3;
    const data = new Uint8ClampedArray(w * h * 4);
    for (let i = 0; i < w * h; i += 1) data.set([(i * 37) % 256, (i * 91) % 256, (i * 53) % 256, 255], i * 4);
    const copy = data.slice();
    applyToPixels(data, samplePro, w);
    const t = curves(samplePro);
    for (let i = 0; i < w * h; i += 1) {
      const weight = vignetteWeight(samplePro.vignette, (i % w) + 0.5, Math.floor(i / w) + 0.5, w, h);
      expect([...data.slice(i * 4, i * 4 + 3)]).toEqual(pixelAt(samplePro, t, copy[i * 4]!, copy[i * 4 + 1]!, copy[i * 4 + 2]!, weight));
    }
  });

  it('HSL และเส้นโค้ง: ส้ม (ผิว) สว่างขึ้น สีเทาไม่เปลี่ยน · เส้นตรง = ไม่ปรับ', () => {
    const skin: Adjust = { ...NEUTRAL, lum: [0, 100, 0, 0, 0, 0, 0, 0] };
    const t = curves(skin);
    const lit = pixel(skin, t, 200, 130, 80);
    expect(lit[1]).toBeGreaterThan(130);
    expect(lit[2]).toBeGreaterThan(80);
    expect(pixel(skin, t, 100, 100, 100)).toEqual([100, 100, 100]);
    expect(pixel(skin, t, 40, 60, 200)).toEqual([40, 60, 200]);
    expect(isNeutral({ ...NEUTRAL, curve: [[0, 0], [255, 255]] })).toBe(true);
    const lifted: Adjust = { ...NEUTRAL, curve: [[0, 30], [255, 255]] };
    expect(pixel(lifted, curves(lifted), 0, 0, 0)).toEqual([30, 30, 30]);
  });

  it('ไม่ปรับ = ภาพเดิม · ขาวดำ = ความสว่างเดิม', () => {
    const data = new Uint8ClampedArray([200, 80, 40, 255, 3, 128, 250, 255]);
    applyToPixels(data, NEUTRAL);
    expect([...data]).toEqual([200, 80, 40, 255, 3, 128, 250, 255]);
    const t = curves({ ...NEUTRAL, saturation: -100 });
    expect(pixel({ ...NEUTRAL, saturation: -100 }, t, 200, 80, 40)).toEqual([103, 103, 103]);
  });

  it('ปรับอัตโนมัติ: ภาพมืดและแคบ → เพิ่มแสงและยืดช่วง', () => {
    const data = new Uint8ClampedArray(4 * 4000);
    for (let i = 0; i < 4000; i += 1) {
      const v = 30 + (i % 60); // 30–89: มืดและไม่เต็มช่วง
      data.set([v, v, v, 255], i * 4);
    }
    const a = autoAdjust(data);
    expect(a.exposure).toBeGreaterThan(0.5);
    expect(a.exposure).toBeLessThanOrEqual(1);
    expect(a.blacks).toBeLessThan(0);
    expect(a.whites).toBe(60);
    expect(a.saturation).toBe(0);
  });

  it('ปรับอัตโนมัติ: ภาพที่สว่างพอดีอยู่แล้วไม่ถูกลดค่าแสง', () => {
    const data = new Uint8ClampedArray(4 * 4000);
    for (let i = 0; i < 4000; i += 1) {
      const v = 20 + ((i * 7) % 220); // กระจายเต็มช่วง ค่ากลางราว 50%
      data.set([v, v, v, 255], i * 4);
    }
    expect(autoAdjust(data).exposure).toBe(0);
  });

  it('ปรับพิกเซลของ canvas ได้ค่าเดียวกับสูตรต่อพิกเซล', () => {
    const t = curves(sample);
    const data = new Uint8ClampedArray([10, 20, 30, 255, 120, 90, 200, 255, 240, 230, 220, 255]);
    applyToPixels(data, sample);
    expect([...data.slice(0, 3)]).toEqual(pixel(sample, t, 10, 20, 30));
    expect([...data.slice(4, 7)]).toEqual(pixel(sample, t, 120, 90, 200));
    expect([...data.slice(8, 11)]).toEqual(pixel(sample, t, 240, 230, 220));
    // ทางที่มี HSL ก็ได้ค่าเดียวกัน
    const full = curves(sampleFull);
    const more = new Uint8ClampedArray([10, 20, 30, 255, 200, 130, 80, 255]);
    applyToPixels(more, sampleFull);
    expect([...more.slice(0, 3)]).toEqual(pixel(sampleFull, full, 10, 20, 30));
    expect([...more.slice(4, 7)]).toEqual(pixel(sampleFull, full, 200, 130, 80));
  });
});
