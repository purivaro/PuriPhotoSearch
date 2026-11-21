import type { EditSettings, UnitRect } from '@/lib/ipc/types';
import { applyToPixels, type Profile } from './adjust';
import { applyLocal, hasLocal } from './local';
import { applyMasks, maskIsActive } from './masks';

/** canvas เทียบกับภาพเดิม: ส่วน `view` ของกรอบหลังปรับเอียงที่ canvas แสดง และขนาดภาพเดิม (หลังหมุนตาม EXIF) */
export interface PreviewFrame {
  view: UnitRect;
  sourceW: number;
  sourceH: number;
  /** canvas นี้คือกรอบหลังตัดทั้งกรอบ (คิด Vignette ได้) */
  whole: boolean;
}

/** มีการปรับเฉพาะส่วนที่มีผล หรือกำลังแสดงบริเวณของ Mask */
export const isMasking = (s: EditSettings, overlay: string | null) => (s.masks ?? []).some(maskIsActive) || overlay !== null;

/**
 * ขั้นพิกเซลของตัวอย่างสด บนพิกเซล RGBA ของ canvas: แสงสี → Texture/Clarity/Dehaze → ปรับเฉพาะส่วน
 * — ลำดับเดียวกับภาพที่บันทึก (`adjust::apply_profile` แล้ว `masks::apply` ของ Rust) · ตรวจว่าได้ผลเดียวกันด้วย
 * `crates/core/tests/preview_parity.rs` (สุ่มค่าการแต่งหลายร้อยชุดเทียบทีละพิกเซล)
 */
export function renderPixels(px: Uint8ClampedArray, width: number, height: number, s: EditSettings, profile: Profile, f: PreviewFrame, overlay: string | null = null): void {
  // รัศมีของ Texture/Clarity/Dehaze และ Mask คิดจากด้านยาวของทั้งกรอบ (ขยายดูบางส่วนก็ได้ผลแบบเดียวกับทั้งภาพ)
  const long = Math.max(width / f.view.w, height / f.view.h);
  applyToPixels(px, s.adjust, f.whole ? width : undefined, profile);
  if (hasLocal(s.adjust)) applyLocal(px, width, height, 4, long, s.adjust);
  if (isMasking(s, overlay)) {
    applyMasks(px, { width, height, view: f.view, sourceW: f.sourceW, sourceH: f.sourceH, settings: s }, s.masks ?? [], long, overlay);
  }
}
