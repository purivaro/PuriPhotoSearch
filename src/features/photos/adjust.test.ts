import { describe, expect, it } from 'vitest';
import { NEUTRAL, applyToPixels, autoAdjust, curves, pixel, type Adjust } from './adjust';

// ค่าเดียวกับ adjust::tests::sample ใน Rust
const sample: Adjust = { exposure: 0.5, contrast: 30, highlights: -40, shadows: 50, whites: 10, blacks: -10, temperature: 20, tint: -10, vibrance: 25, saturation: 10 };

describe('แสงและสี (สูตรเดียวกับ Rust)', () => {
  it('ได้ค่าเดียวกับ adjust::tests::matches_the_ui_formula ทุกพิกเซล', () => {
    const t = curves(sample);
    expect(pixel(sample, t, 10, 20, 30)).toEqual([11, 30, 47]);
    expect(pixel(sample, t, 120, 90, 200)).toEqual([148, 108, 251]);
    expect(pixel(sample, t, 240, 230, 220)).toEqual([255, 255, 252]);
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
  });
});
