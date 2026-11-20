import type { Id, WatermarkAnchor, WatermarkInfo, WatermarkSpec } from '@/lib/ipc/types';

/** ลายน้ำที่ผู้ใช้ตั้งไว้ในหน้าต่างส่งออก (จำไว้ในเครื่อง) */
export interface WatermarkChoice {
  enabled: boolean;
  id: Id | null;
  anchor: WatermarkAnchor;
  /** ด้านยาวของโลโก้ เป็น % ของด้านสั้นของภาพ */
  scale: number;
  /** ความทึบ % */
  opacity: number;
}

// ค่าเริ่มต้นเดียวกับ PuriSlim
export const DEFAULT_WATERMARK_CHOICE: WatermarkChoice = { enabled: false, id: null, anchor: 'bottom_right', scale: 25, opacity: 80 };

export const SCALE_RANGE = { min: 5, max: 100 } as const;
export const OPACITY_RANGE = { min: 5, max: 100 } as const;

/** เก้าตำแหน่งเรียงตามตาราง 3×3 (แถวบน → ล่าง) */
export const ANCHORS: { value: WatermarkAnchor; label: string }[] = [
  { value: 'top_left', label: 'ซ้ายบน' },
  { value: 'top_center', label: 'กลางบน' },
  { value: 'top_right', label: 'ขวาบน' },
  { value: 'center_left', label: 'ซ้ายกลาง' },
  { value: 'center', label: 'ตรงกลาง' },
  { value: 'center_right', label: 'ขวากลาง' },
  { value: 'bottom_left', label: 'ซ้ายล่าง' },
  { value: 'bottom_center', label: 'กลางล่าง' },
  { value: 'bottom_right', label: 'ขวาล่าง' },
];

export function anchorLabel(anchor: WatermarkAnchor): string {
  return ANCHORS.find((a) => a.value === anchor)?.label ?? '';
}

/** ระยะขอบเป็นสัดส่วนของด้านสั้น (Rust: media::WATERMARK_MARGIN) */
const MARGIN = 0.03;

/**
 * ตำแหน่งและขนาดของลายน้ำบนภาพ width×height — สูตรเดียวกับ `media::watermark_rect` ใน Rust
 * ตัวอย่างในหน้าต่างจึงตรงกับไฟล์ที่ได้
 */
export function watermarkRect(width: number, height: number, logoW: number, logoH: number, scale: number, anchor: WatermarkAnchor) {
  const short = Math.min(width, height);
  const ratio = Math.max(short * scale, 1) / Math.max(logoW, logoH, 1);
  const w = Math.min(Math.max(Math.round(logoW * ratio), 1), Math.max(width, 1));
  const h = Math.min(Math.max(Math.round(logoH * ratio), 1), Math.max(height, 1));
  const margin = Math.round(short * MARGIN);
  const index = ANCHORS.findIndex((a) => a.value === anchor);
  const place = (at: number, size: number, room: number) =>
    at === 0 ? Math.min(margin, Math.max(room - size, 0)) : at === 1 ? Math.floor(Math.max(room - size, 0) / 2) : Math.max(room - size - margin, 0);
  return { x: place(index % 3, w, width), y: place(Math.floor(index / 3), h, height), width: w, height: h };
}

/** ลายน้ำที่ส่งให้ Rust: null = ไม่ใส่ · error = เปิดไว้แต่ยังเลือกไม่ครบ */
export function toWatermarkSpec(choice: WatermarkChoice, available: WatermarkInfo[] | undefined): { spec: WatermarkSpec | null } | { error: string } {
  if (!choice.enabled) return { spec: null };
  if (!available) return { error: 'กำลังโหลดรายการลายน้ำ…' };
  if (available.length === 0) return { error: 'ยังไม่มีลายน้ำ เพิ่มไฟล์โลโก้ก่อน' };
  if (!choice.id || !available.some((w) => w.id === choice.id)) return { error: 'เลือกลายน้ำที่จะใช้' };
  return { spec: { id: choice.id, anchor: choice.anchor, scale: choice.scale, opacity: choice.opacity } };
}

/** ตัวแทนค่าสำหรับเทียบว่าแผนตรงกับที่เลือกอยู่ (ลำดับ key ที่ Rust ส่งกลับไม่เหมือนที่ส่งไป) */
export function watermarkKeyOf(spec: WatermarkSpec | null): string {
  return spec ? `${spec.id}:${spec.anchor}:${spec.scale}:${spec.opacity}` : 'none';
}

const clamp = (value: unknown, range: { min: number; max: number }, fallback: number) =>
  typeof value === 'number' && Number.isInteger(value) ? Math.min(range.max, Math.max(range.min, value)) : fallback;

/** อ่านค่าที่เคยเก็บไว้ ถ้าเสียหรือไม่ครบใช้ค่าเริ่มต้น */
export function parseWatermarkChoice(raw: string | null): WatermarkChoice {
  if (!raw) return DEFAULT_WATERMARK_CHOICE;
  try {
    const v = JSON.parse(raw) as Partial<WatermarkChoice>;
    const d = DEFAULT_WATERMARK_CHOICE;
    return {
      enabled: v.enabled === true,
      id: typeof v.id === 'string' ? v.id : null,
      anchor: ANCHORS.some((a) => a.value === v.anchor) ? (v.anchor as WatermarkAnchor) : d.anchor,
      scale: clamp(v.scale, SCALE_RANGE, d.scale),
      opacity: clamp(v.opacity, OPACITY_RANGE, d.opacity),
    };
  } catch {
    return DEFAULT_WATERMARK_CHOICE;
  }
}
