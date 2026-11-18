import { describe, expect, it } from 'vitest';
import { boxFromDrag, boxToScreen, clampPan, fitScale, imageRect, isUsableBox, toNormalized, zoomAround } from './geometry';

const container = { w: 1000, h: 600 };
const landscape = { w: 6000, h: 4000 };
const portrait = { w: 4000, h: 6000 };

describe('สี่เหลี่ยมภาพ', () => {
  it('พอดีหน้าต่างและอยู่กลาง', () => {
    expect(fitScale(container, landscape)).toBeCloseTo(0.15);
    expect(imageRect(container, landscape, 1, { x: 0, y: 0 })).toEqual({ x: 50, y: 0, w: 900, h: 600 });
    const p = imageRect(container, portrait, 1, { x: 0, y: 0 });
    expect(p.w).toBeCloseTo(400);
    expect(p.x).toBeCloseTo(300);
  });

  it('ภาพเล็กกว่าพื้นที่ถูกตรึงกลาง ภาพใหญ่เลื่อนได้ไม่เกินขอบ', () => {
    expect(clampPan(container, landscape, 1, { x: 500, y: 500 })).toEqual({ x: 0, y: 0 });
    const pan = clampPan(container, landscape, 2, { x: 9999, y: -9999 });
    expect(pan).toEqual({ x: 400, y: -300 });
    const rect = imageRect(container, landscape, 2, pan);
    expect(rect.x).toBeCloseTo(0);
    expect(rect.y + rect.h).toBeCloseTo(600);
  });

  it('zoom รอบตัวชี้: จุดใต้ตัวชี้อยู่ที่เดิม', () => {
    const focus = { x: 700, y: 200 };
    const before = imageRect(container, landscape, 1, { x: 0, y: 0 });
    const uv = toNormalized(before, focus);
    const next = zoomAround(container, landscape, 1, { x: 0, y: 0 }, 3, focus);
    const after = imageRect(container, landscape, next.zoom, next.pan);
    const screen = boxToScreen(after, { ...uv, w: 0, h: 0 });
    expect(screen.x).toBeCloseTo(focus.x, 3);
    expect(screen.y).toBeCloseTo(focus.y, 3);
  });
});

describe('กรอบใบหน้า', () => {
  it('ลากทิศใดก็ได้กรอบเดียวกัน และตัดให้อยู่ในภาพ', () => {
    const rect = imageRect(container, landscape, 1, { x: 0, y: 0 });
    const a = toNormalized(rect, { x: 275, y: 150 });
    const b = toNormalized(rect, { x: 500, y: 450 });
    expect(boxFromDrag(a, b)).toEqual(boxFromDrag(b, a));
    expect(boxFromDrag(a, b)).toEqual({ x: 0.25, y: 0.25, w: 0.25, h: 0.5 });
    expect(toNormalized(rect, { x: -40, y: 9999 })).toEqual({ x: 0, y: 1 });
  });

  it('กรอบสัดส่วนเดียวกันชี้จุดเดิมบนภาพทุกระดับ zoom และทุกขนาดหน้าต่าง', () => {
    const box = { x: 0.4, y: 0.3, w: 0.1, h: 0.2 };
    for (const size of [container, { w: 640, h: 700 }, { w: 1800, h: 500 }]) {
      for (const zoom of [1, 1.7, 4]) {
        const rect = imageRect(size, portrait, zoom, clampPan(size, portrait, zoom, { x: 37, y: -80 }));
        const screen = boxToScreen(rect, box);
        const back = toNormalized(rect, { x: screen.x, y: screen.y });
        expect(back.x).toBeCloseTo(box.x, 6);
        expect(back.y).toBeCloseTo(box.y, 6);
        expect(screen.w / rect.w).toBeCloseTo(box.w, 6);
      }
    }
  });

  it('การคลิกพลาดไม่นับเป็นกรอบ', () => {
    expect(isUsableBox({ x: 0.5, y: 0.5, w: 0.002, h: 0.2 })).toBe(false);
    expect(isUsableBox({ x: 0.5, y: 0.5, w: 0.05, h: 0.06 })).toBe(true);
  });
});
