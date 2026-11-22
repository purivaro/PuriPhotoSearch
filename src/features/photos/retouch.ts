import type { FaceBox, Retouch } from '@/lib/ipc/types';

/**
 * รีทัชใบหน้า (DECISIONS ข้อ 31) ฝั่ง UI — ภาพคิดใน Rust (`crates/core/src/retouch.rs`) ทั้งตัวอย่างสดและไฟล์ที่บันทึก
 */

export type RetouchKey = 'smooth' | 'wrinkles' | 'slim' | 'thin';

/** ระดับสำเร็จรูป ไม่/เบา/กลาง/มาก (เบา กลาง มาก เท่ากับ retouch::LEVELS ใน Rust) */
export const RETOUCH_LEVELS: { label: string; value: number }[] = [
  { label: 'ไม่', value: 0 },
  { label: 'เบา', value: 25 },
  { label: 'กลาง', value: 50 },
  { label: 'มาก', value: 80 },
];

export const RETOUCH_ITEMS: { key: RetouchKey; label: string; description: string }[] = [
  { key: 'smooth', label: 'ผิวเนียน', description: 'ลดรูขุมขนและผิวไม่เรียบ ตา คิ้ว ปากยังคม' },
  { key: 'wrinkles', label: 'ลดริ้วรอย', description: 'ริ้วรอยและร่องเงาบนหน้าจางลง' },
  { key: 'slim', label: 'หน้าเรียว', description: 'กรามถึงคางเรียวลง' },
  { key: 'thin', label: 'หน้าผอม', description: 'แก้มเล็กลง' },
];

export const NO_RETOUCH: Retouch = { faces: [], smooth: 0, wrinkles: 0, slim: 0, thin: 0, body: 0 };

/** ตัวผอม (DECISIONS ข้อ 41) — ระดับเดียวกับรีทัชใบหน้า */
export const bodyAmount = (r: Retouch | null | undefined): number => r?.body ?? 0;

/** มีผลกับภาพ (ตัวผอมไม่ต้องมีกรอบใบหน้า) */
export const retouchIsActive = (r: Retouch | null | undefined): r is Retouch =>
  !!r && ((r.faces.length > 0 && RETOUCH_ITEMS.some(({ key }) => r[key] > 0)) || bodyAmount(r) > 0);

/** กรอบของใบหน้าทุกหน้าของรูป (บนภาพเดิม) */
export const faceBoxes = (faces: FaceBox[]): Retouch['faces'] => faces.map((f) => [f.sourceBbox.x, f.sourceBbox.y, f.sourceBbox.w, f.sourceBbox.h]);

/** สรุปเป็นภาษาผู้ใช้ เช่น "ผิวเนียน 50 · หน้าเรียว 25" */
export function describeRetouch(r: Retouch | null | undefined): string | null {
  if (!retouchIsActive(r)) return null;
  const parts = r.faces.length > 0 ? RETOUCH_ITEMS.filter(({ key }) => r[key] > 0).map(({ key, label }) => `${label} ${r[key]}`) : [];
  if (bodyAmount(r) > 0) parts.push(`ตัวผอม ${bodyAmount(r)}`);
  return parts.join(' · ');
}
