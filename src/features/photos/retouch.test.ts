import { describe, expect, it } from 'vitest';
import { NO_RETOUCH, bodyAmount, describeRetouch, retouchIsActive } from './retouch';

describe('ตัวผอม (DECISIONS ข้อ 41)', () => {
  it('มีผลแม้ไม่มีกรอบใบหน้า และสรุปเป็นภาษาผู้ใช้', () => {
    const body = { ...NO_RETOUCH, body: 50 };
    expect(retouchIsActive(body)).toBe(true);
    expect(describeRetouch(body)).toBe('ตัวผอม 50');
  });

  it('รวมกับรีทัชใบหน้า · รีทัชใบหน้าไม่มีกรอบใบหน้า = ไม่มีผล', () => {
    const face: [number, number, number, number] = [0.3, 0.2, 0.2, 0.3];
    expect(describeRetouch({ ...NO_RETOUCH, faces: [face], smooth: 25, body: 80 })).toBe('ผิวเนียน 25 · ตัวผอม 80');
    expect(retouchIsActive({ ...NO_RETOUCH, smooth: 25 })).toBe(false);
    expect(describeRetouch({ ...NO_RETOUCH, smooth: 25, body: 25 })).toBe('ตัวผอม 25');
  });

  it('การแต่งรุ่นก่อนไม่มีค่าตัวผอม = 0', () => {
    const { body: _, ...old } = NO_RETOUCH;
    expect(bodyAmount(old)).toBe(0);
    expect(retouchIsActive(old)).toBe(false);
  });
});
