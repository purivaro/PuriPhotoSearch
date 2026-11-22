import { describe, expect, it } from 'vitest';
import type { EditSettings } from '@/lib/ipc/types';
import { NEUTRAL } from './adjust';
import { brushToSource, faceRemoved, personRect, sourceToView, viewToSource } from './removals';

const plain: EditSettings = { rotate: 0, straighten: 0, crop: null, adjust: NEUTRAL };
const FULL = { x: 0, y: 0, w: 1, h: 1 };
const near = (a: number[], b: number[]) => a.forEach((v, i) => expect(v).toBeCloseTo(b[i] ?? Number.NaN, 6));

describe('ลบคน/สิ่งของ: แปลงกรอบ (DECISIONS ข้อ 42)', () => {
  it('ไม่ได้ตัด/หมุน = กรอบเดียวกัน · ตัดแล้วกรอบบนจอเป็นสัดส่วนของส่วนที่เห็น', () => {
    near(viewToSource(600, 400, plain, FULL, { x: 0.2, y: 0.3, w: 0.1, h: 0.4 }), [0.2, 0.3, 0.1, 0.4]);
    const crop = { x: 0.5, y: 0.5, w: 0.5, h: 0.5 };
    near(viewToSource(600, 400, { ...plain, crop }, crop, { x: 0, y: 0, w: 0.5, h: 0.5 }), [0.5, 0.5, 0.25, 0.25]);
  });

  it('หมุน 90° แล้วกลับไปกลับมาได้กรอบเดิม', () => {
    const turned: EditSettings = { ...plain, rotate: 90 };
    const rect = viewToSource(600, 400, turned, FULL, { x: 0.1, y: 0.2, w: 0.3, h: 0.2 });
    const back = sourceToView(600, 400, turned, FULL, rect);
    near([back.x, back.y, back.w, back.h], [0.1, 0.2, 0.3, 0.2]);
  });

  it('เส้นที่ระบาย: จุดและรัศมีบนภาพเดิม (รัศมีเป็นสัดส่วนของด้านยาว) และกรอบที่ครอบ', () => {
    // ภาพ 600×400 ไม่ได้ตัด · รัศมี 0.05 ของความกว้างที่แสดง = 30 พิกเซล = 0.05 ของด้านยาว
    const out = brushToSource(600, 400, plain, FULL, [{ x: 0.5, y: 0.5 }, { x: 0.6, y: 0.5 }], 0.05);
    expect(out?.brush).toHaveLength(2);
    near(out?.brush?.[0] ?? [], [0.5, 0.5, 0.05]);
    near(out?.rect ?? [], [0.45, 0.425, 0.2, 0.15]);
    // ตัดภาพแล้ว (เห็นครึ่งซ้ายบน): จุดกลางที่แสดง = จุด 0.25 บนภาพเดิม และรัศมีเล็กลงครึ่งหนึ่ง
    const crop = { x: 0, y: 0, w: 0.5, h: 0.5 };
    const cut = brushToSource(600, 400, { ...plain, crop }, crop, [{ x: 0.5, y: 0.5 }], 0.05);
    near(cut?.brush?.[0] ?? [], [0.25, 0.25, 0.025]);
    expect(brushToSource(600, 400, plain, FULL, [], 0.05)).toBeNull();
  });

  it('กรอบของคนจากใบหน้า และใบหน้าที่ลบไปแล้ว', () => {
    const face = { x: 0.5, y: 0.2, w: 0.05, h: 0.06 };
    const rect = personRect(face);
    near(rect, [0.45, 0.179, 0.15, 0.441]);
    expect(faceRemoved(face, [{ id: 'a', rect }])).toBe(true);
    expect(faceRemoved({ ...face, x: 0.1 }, [{ id: 'a', rect }])).toBe(false);
    // ไม่เลยขอบภาพ
    const edge = personRect({ x: 0.98, y: 0.9, w: 0.05, h: 0.06 });
    expect(edge[0] + edge[2]).toBeLessThanOrEqual(1);
    expect(edge[1] + edge[3]).toBeLessThanOrEqual(1);
  });
});
