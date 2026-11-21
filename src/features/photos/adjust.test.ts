import { describe, expect, it } from 'vitest';
import { NEUTRAL, curveFunction, histogram, isNeutral, sameAdjust, type Adjust } from './adjust';

// สูตรแสงสีอยู่ใน Rust ชุดเดียว (DECISIONS ข้อ 30) — ไฟล์นี้ทดสอบเฉพาะตัวช่วยของ UI
describe('ตัวช่วยของ UI แต่งภาพ', () => {
  it('ไม่ได้แต่ง: ค่าเริ่มต้น และค่าที่บันทึกก่อนมีช่องใหม่ (ไม่มี texture) ก็ถือว่าไม่ได้แต่ง', () => {
    expect(isNeutral(NEUTRAL)).toBe(true);
    const old = { ...NEUTRAL } as Partial<Adjust>;
    delete old.texture;
    expect(isNeutral(old as Adjust)).toBe(true);
    expect(isNeutral({ ...NEUTRAL, texture: -10 })).toBe(false);
    expect(isNeutral({ ...NEUTRAL, lum: [0, 20, 0, 0, 0, 0, 0, 0] })).toBe(false);
    expect(isNeutral({ ...NEUTRAL, curve: [[0, 0], [128, 128], [255, 255]] })).toBe(true);
  });

  it('เทียบค่าแสงสีโดยไม่ขึ้นกับลำดับของช่อง', () => {
    const reordered = Object.fromEntries(Object.entries({ ...NEUTRAL, exposure: 0.5 }).reverse()) as unknown as Adjust;
    expect(sameAdjust(reordered, { ...NEUTRAL, exposure: 0.5 })).toBe(true);
    expect(sameAdjust(reordered, NEUTRAL)).toBe(false);
  });

  it('เส้นโค้งที่วาดในแผงผ่านทุกจุดและไม่แกว่งเกินจุด (สูตรเดียวกับ Spline ของ Rust)', () => {
    const f = curveFunction([
      [0, 0],
      [64, 40],
      [192, 220],
      [255, 255],
    ]);
    expect(f(64 / 255)).toBeCloseTo(40 / 255, 9);
    expect(f(192 / 255)).toBeCloseTo(220 / 255, 9);
    let last = -1;
    for (let i = 0; i <= 255; i += 1) {
      const y = f(i / 255);
      expect(y).toBeGreaterThanOrEqual(last);
      last = y;
    }
    expect(curveFunction([])(0.3)).toBe(0.3);
  });

  it('ฮิสโตแกรมนับทุกพิกเซลที่สี่ของภาพ RGBA', () => {
    const px = new Uint8ClampedArray(8 * 4);
    for (let i = 0; i < 8; i += 1) px.set([i * 10, 100, 200, 255], i * 4);
    const h = histogram(px);
    expect(h.r[0]).toBe(1);
    expect(h.r[40]).toBe(1);
    expect(h.g[100]).toBe(2);
    expect(h.b[200]).toBe(2);
  });
});
