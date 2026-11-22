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

/** กรอบที่ลากเล็กเกินไป (พิกเซลบนจอ) ไม่นับ — คลิกพลาด */
export const MIN_DRAG_PX = 10;
