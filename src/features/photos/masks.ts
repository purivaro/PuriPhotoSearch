import type { EditMask } from '@/lib/ipc/types';
import { isNeutral } from './adjust';

/**
 * ปรับเฉพาะส่วน (DECISIONS ข้อ 28) ฝั่ง UI — บริเวณ น้ำหนัก และการผสมคิดใน Rust (`crates/core/src/masks.rs`)
 * ทั้งตัวอย่างสดและไฟล์ที่บันทึก (DECISIONS ข้อ 30)
 */

/** จำนวนบริเวณสูงสุดต่อรูป (เท่ากับ masks::MAX_MASKS ใน Rust) */
export const MAX_MASKS = 10;

/** มีผลกับภาพ: ความแรงมากกว่า 0 และปรับค่าอย่างน้อยหนึ่งอย่าง */
export const maskIsActive = (m: EditMask) => m.amount > 0 && !isNeutral(m.adjust);

/** บริเวณ "ตัวคน (AI)" — แอปหาตัวคนจากรูปเอง (DECISIONS ข้อ 40) */
export const isSubjectMask = (m: EditMask) => m.area.kind === 'subject';

/**
 * เพิ่ม Mask ของ preset ให้รูป: แทนอันที่รหัสเดียวกัน (ใช้ preset ซ้ำไม่ซ้อน) Mask อื่นของรูปอยู่ครบ
 * เกินจำนวนที่รับได้ = ตัดของเดิมที่เก่าที่สุด (เหมือน `EditParts::merge` ใน Rust)
 */
export function addMasks(existing: EditMask[], added: EditMask[]): EditMask[] {
  if (added.length === 0) return existing;
  const kept = existing.filter((m) => added.every((a) => a.id !== m.id));
  const all = [...kept, ...added];
  return all.slice(Math.max(0, all.length - MAX_MASKS));
}
