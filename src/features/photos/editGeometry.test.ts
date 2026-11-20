import { describe, expect, it } from 'vitest';
import { boxToFrame, cutOut, dragCrop, fitAspect, outputSize, rotateCrop, toFrame, turn } from './editGeometry';

const near = (a: number, b: number) => Math.abs(a - b) < 1e-9;

describe('คณิตของหน้าแต่งภาพ (ค่าเดียวกับ edit::tests ใน Rust)', () => {
  it('หมุน 90° ตามเข็ม: มุมซ้ายบนไปขวาบน ภาพแนวนอนกลายเป็นแนวตั้ง', () => {
    const s = { rotate: 90 as const, straighten: 0, crop: null };
    expect(outputSize(400, 300, s)).toEqual({ w: 300, h: 400 });
    const p = toFrame(400, 300, s, 0, 0);
    expect(near(p.x, 1) && near(p.y, 0)).toBe(true);
    expect(turn(0, -1)).toBe(270);
    expect(turn(270, 1)).toBe(0);
  });

  it('ปรับเอียงคงสัดส่วนเดิมและเล็กลง', () => {
    const { w, h } = outputSize(1000, 500, { rotate: 0, straighten: 10, crop: null });
    expect(Math.abs(w / h - 2)).toBeLessThan(0.01);
    expect(w).toBeLessThan(1000);
  });

  it('กรอบใบหน้าตามการหมุนและตัด: ใบหน้าที่เหลือในกรอบไม่ถึงครึ่งถือว่าถูกตัดออก', () => {
    const s = { rotate: 90 as const, straighten: 0, crop: { x: 0, y: 0, w: 1, h: 0.5 } };
    const left = boxToFrame(400, 300, s, { x: 0.1, y: 0.2, w: 0.1, h: 0.2 });
    expect(near(left.x, 0.6) && near(left.y, 0.1) && near(left.w, 0.2) && near(left.h, 0.1)).toBe(true);
    expect(cutOut(left, s.crop)).toBe(false);
    expect(cutOut(boxToFrame(400, 300, s, { x: 0.7, y: 0.2, w: 0.1, h: 0.2 }), s.crop)).toBe(true);
  });

  it('หมุนกรอบตัดไปพร้อมภาพ ไปกลับได้รูปเดิม', () => {
    const c = { x: 0.1, y: 0.2, w: 0.5, h: 0.3 };
    const back = rotateCrop(rotateCrop(c, 1), -1)!;
    expect(near(back.x, c.x) && near(back.y, c.y) && near(back.w, c.w) && near(back.h, c.h)).toBe(true);
    const once = rotateCrop(c, 1)!;
    expect(near(once.x, 0.5) && near(once.y, 0.1) && near(once.w, 0.3) && near(once.h, 0.5)).toBe(true);
  });

  it('เลือกสัดส่วน: กรอบใหญ่สุดในภาพ และได้สัดส่วนพิกเซลตามที่เลือก', () => {
    // กรอบ 3:2 เลือก 1:1 → กว้าง 2/3 ของภาพ สูงเต็ม
    const c = fitAspect(null, 1, 1.5);
    expect(near(c.w, 2 / 3) && near(c.h, 1) && near(c.x, 1 / 6)).toBe(true);
    expect(near((c.w * 1.5) / c.h, 1)).toBe(true);
  });

  it('ลากกรอบไม่หลุดภาพ ไม่เล็กเกิน และล็อกสัดส่วนได้', () => {
    const start = { x: 0.2, y: 0.2, w: 0.5, h: 0.5 };
    const moved = dragCrop(start, 'move', 0.9, -0.9, null, 1.5);
    expect(near(moved.x, 0.5) && near(moved.y, 0)).toBe(true);
    const tiny = dragCrop(start, 'se', -1, -1, null, 1.5);
    expect(tiny.w).toBeGreaterThanOrEqual(0.02 - 1e-9);
    // ล็อก 1:1 บนกรอบ 3:2: h = w × 1.5
    const locked = dragCrop(start, 'se', 0.1, 0, 1, 1.5);
    expect(near(locked.h, locked.w * 1.5)).toBe(true);
    expect(locked.x + locked.w).toBeLessThanOrEqual(1 + 1e-9);
    expect(locked.y + locked.h).toBeLessThanOrEqual(1 + 1e-9);
  });
});
