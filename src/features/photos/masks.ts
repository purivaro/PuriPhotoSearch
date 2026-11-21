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
