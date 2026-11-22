import { describe, expect, it } from 'vitest';
import type { EditSettings } from '@/lib/ipc/types';
import { FULL, IDENTITY } from './editGeometry';
import { linearFromDrag, linearShape, mapper, moveLinear, moveRadial, radialFromDrag, radialShape } from './gradients';

// ภาพเดิม 300×200 แสดงเต็มกรอบที่ 600×400 พิกเซลบนจอ
const plain = mapper(300, 200, IDENTITY, FULL, 600, 400);
const close = (a: number, b: number, digits = 6) => expect(a).toBeCloseTo(b, digits);

describe('Mask ไล่ระดับ / วงกลม (DECISIONS ข้อ 45)', () => {
  it('maps screen points to the photo and back', () => {
    const [u, v] = plain.toSource({ x: 150, y: 300 });
    close(u, 0.25);
    close(v, 0.75);
    const p = plain.toScreen([u, v]);
    close(p.x, 150);
    close(p.y, 300);
  });

  it('makes a linear gradient from a drag: full where it starts, gone where it ends', () => {
    expect(linearFromDrag(plain, { x: 300, y: 0 }, { x: 302, y: 3 })).toBeNull();
    const g = linearFromDrag(plain, { x: 300, y: 0 }, { x: 300, y: 200 });
    expect(g).toEqual({ kind: 'linear', full: [0.5, 0], zero: [0.5, 0.5] });
    // เส้นทั้งสามตั้งฉากกับแนวไล่บนจอ
    const s = linearShape(plain, g!, 600, 400);
    for (const [a, b] of s.lines) close(a.y, b.y, 9);
  });

  it('makes a round circle from a drag even on a wide photo', () => {
    const r = radialFromDrag(plain, { x: 300, y: 200 }, { x: 400, y: 300 }, 600, 400);
    expect(r.center).toEqual([0.5, 0.5]);
    // 100 พิกเซลบนจอ = 50 พิกเซลของภาพเดิม → 50/300 ของความกว้าง และ 50/200 ของความสูง (วงกลมจริง)
    close(r.radius[0], 50 / 300);
    close(r.radius[1], 50 / 200);
    expect(r.angle).toBe(0);
    expect(r.feather).toBe(50);
    // กดเฉย ๆ = วงขนาดตั้งต้น
    expect(radialFromDrag(plain, { x: 300, y: 200 }, { x: 301, y: 200 }, 600, 400).radius[0]).toBeGreaterThan(0.1);
  });

  it('draws the circle where it is on the photo after the photo is turned', () => {
    const turned: EditSettings = { ...IDENTITY, rotate: 90 };
    // ภาพหมุนแล้วแสดงเป็น 400×600 บนจอ
    const m = mapper(300, 200, turned, FULL, 400, 600);
    const r = radialFromDrag(m, { x: 200, y: 300 }, { x: 260, y: 360 }, 400, 600);
    close(r.center[0], 0.5);
    close(r.center[1], 0.5);
    const s = radialShape(m, r);
    // ขอบขวาของวงบนจออยู่ห่างจากกลาง 60 พิกเซลตามที่ลาก
    close(Math.hypot(s.xHandle.x - s.center.x, s.xHandle.y - s.center.y), 60, 4);
    close(Math.hypot(s.yHandle.x - s.center.x, s.yHandle.y - s.center.y), 60, 4);
  });

  it('moves and turns a circle with its handles', () => {
    const r = radialFromDrag(plain, { x: 300, y: 200 }, { x: 400, y: 300 }, 600, 400);
    const moved = moveRadial(plain, r, 'center', { x: 320, y: 210 }, { from: { x: 300, y: 200 }, center: r.center });
    close(moved.center[0], 320 / 600);
    close(moved.center[1], 210 / 400);
    // ลากขอบแกนนอนลงไปใต้กลางวง = หมุน 90° และรัศมีแนวนั้นตามระยะ
    const turned = moveRadial(plain, r, 'x', { x: 300, y: 360 });
    close(turned.angle, 90);
    close(turned.radius[0], 80 / 300);
    // แกนตั้ง: ระยะตั้งฉากกับแกนนอน
    const taller = moveRadial(plain, r, 'y', { x: 330, y: 380 });
    close(taller.radius[1], 90 / 200);
    expect(taller.radius[0]).toBe(r.radius[0]);
  });

  it('moves a linear gradient point by point or as a whole', () => {
    const g = linearFromDrag(plain, { x: 300, y: 0 }, { x: 300, y: 200 })!;
    const whole = moveLinear(plain, g, 'mid', { x: 360, y: 140 }, { from: { x: 300, y: 100 }, area: g });
    expect(whole.full).toEqual([0.6, 0.1]);
    expect(whole.zero).toEqual([0.6, 0.6]);
    expect(moveLinear(plain, g, 'zero', { x: 300, y: 380 }).zero).toEqual([0.5, 0.95]);
    // ลากจุดหนึ่งทับอีกจุด = คงเดิม (ไม่รู้แนว)
    expect(moveLinear(plain, g, 'zero', { x: 300, y: 0 })).toBe(g);
  });
});
