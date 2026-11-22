import type { EditSettings, Removal, UnitRect } from '@/lib/ipc/types';
import { boxToFrame, fromFrame } from './editGeometry';

/**
 * ลบคน/สิ่งของ (DECISIONS ข้อ 42) ฝั่ง UI: แปลงกรอบระหว่างภาพที่แสดง (ส่วนที่เห็นหลังตัด/หมุน) กับภาพเดิม และกรอบของคนจากใบหน้า
 * — ภาพคิดใน Rust (`crates/core/src/remove.rs`)
 */
export type Rect4 = Removal['rect'];

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

/** กรอบที่ลากบนภาพที่แสดง (สัดส่วนของส่วนที่เห็น `view`) → กรอบบนภาพเดิมที่ครอบมุมทั้งสี่ (ภาพหมุน/เอียงก็ยังครอบครบ) */
export function viewToSource(width: number, height: number, s: EditSettings, view: UnitRect, box: UnitRect): Rect4 {
  const corners: [number, number][] = [
    [box.x, box.y],
    [box.x + box.w, box.y],
    [box.x, box.y + box.h],
    [box.x + box.w, box.y + box.h],
  ];
  const pts = corners.map(([cx, cy]) => fromFrame(width, height, s, view.x + cx * view.w, view.y + cy * view.h));
  const x0 = clamp01(Math.min(...pts.map((p) => p.x)));
  const y0 = clamp01(Math.min(...pts.map((p) => p.y)));
  const x1 = clamp01(Math.max(...pts.map((p) => p.x)));
  const y1 = clamp01(Math.max(...pts.map((p) => p.y)));
  return [x0, y0, x1 - x0, y1 - y0];
}

/** กรอบบนภาพเดิม → บนภาพที่แสดง (สัดส่วนของส่วนที่เห็น — อาจเลยขอบเมื่อถูกตัดออก) */
export function sourceToView(width: number, height: number, s: EditSettings, view: UnitRect, rect: Rect4): UnitRect {
  const b = boxToFrame(width, height, s, { x: rect[0], y: rect[1], w: rect[2], h: rect[3] });
  return { x: (b.x - view.x) / view.w, y: (b.y - view.y) / view.h, w: b.w / view.w, h: b.h / view.h };
}

/** กรอบของทั้งตัวคนจากกรอบใบหน้า (บนภาพเดิม): กว้างราวสามเท่าของหน้า ลงไปถึงราวเจ็ดเท่าของความสูงหน้า (ไม่เกินขอบภาพ) */
export function personRect(face: UnitRect): Rect4 {
  const x0 = clamp01(face.x - face.w);
  const x1 = clamp01(face.x + face.w * 2);
  const y0 = clamp01(face.y - face.h * 0.35);
  const y1 = clamp01(face.y + face.h * 7);
  return [x0, y0, x1 - x0, y1 - y0];
}

/** ใบหน้านี้อยู่ในส่วนที่ลบแล้ว (กลางหน้าอยู่ในกรอบใดกรอบหนึ่ง) */
export function faceRemoved(face: UnitRect, removals: Removal[]): boolean {
  const [cx, cy] = [face.x + face.w / 2, face.y + face.h / 2];
  return removals.some(({ rect: [x, y, w, h] }) => cx >= x && cx <= x + w && cy >= y && cy <= y + h);
}

/** จุดของเส้นที่ระบายได้มากสุดต่อส่วน (เท่ากับ remove::MAX_BRUSH_POINTS ใน Rust) */
export const MAX_BRUSH_POINTS = 2000;

/**
 * เส้นที่ระบายบนภาพที่แสดง (จุดเป็นสัดส่วนของส่วนที่เห็น · `radius` เป็นสัดส่วนของความกว้างที่แสดง) → จุดบนภาพเดิม
 * [x, y, รัศมีเป็นสัดส่วนของด้านยาว] + กรอบที่ครอบส่วนที่ระบาย · ไม่มีจุด = null
 */
export function brushToSource(
  width: number,
  height: number,
  s: EditSettings,
  view: UnitRect,
  points: { x: number; y: number }[],
  radius: number,
): Pick<Removal, 'rect' | 'brush'> | null {
  if (points.length === 0) return null;
  const long = Math.max(width, height, 1);
  const toSource = (x: number, y: number) => fromFrame(width, height, s, view.x + x * view.w, view.y + y * view.h);
  // จุดมากเกิน: เก็บทุก ๆ k จุด (เส้นยาวมาก ๆ) รวมจุดสุดท้าย
  const step = Math.ceil(points.length / MAX_BRUSH_POINTS);
  const kept = points.filter((_, i) => i % step === 0 || i === points.length - 1).slice(0, MAX_BRUSH_POINTS);
  const brush = kept.map((p): [number, number, number] => {
    const a = toSource(p.x, p.y);
    const b = toSource(p.x + radius, p.y);
    const r = Math.hypot((b.x - a.x) * width, (b.y - a.y) * height) / long;
    return [a.x, a.y, Math.min(Math.max(r, 1e-4), 0.3)];
  });
  const xs0 = brush.map(([x, , r]) => x - (r * long) / width);
  const xs1 = brush.map(([x, , r]) => x + (r * long) / width);
  const ys0 = brush.map(([, y, r]) => y - (r * long) / height);
  const ys1 = brush.map(([, y, r]) => y + (r * long) / height);
  const x0 = clamp01(Math.min(...xs0));
  const y0 = clamp01(Math.min(...ys0));
  const x1 = clamp01(Math.max(...xs1));
  const y1 = clamp01(Math.max(...ys1));
  return { rect: [x0, y0, Math.max(x1 - x0, 0.005), Math.max(y1 - y0, 0.005)], brush };
}

/** กรอบที่ลากเล็กเกินไป (พิกเซลบนจอ) ไม่นับ — คลิกพลาด */
export const MIN_DRAG_PX = 10;
