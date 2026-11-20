import type { ExportResize } from '@/lib/ipc/types';

export type ResizePreset = 'original' | '2048' | '1600' | '1080' | 'custom' | 'max';

/** ตัวเลือกขนาดของสำเนาตามที่ผู้ใช้กรอก (เก็บค่าที่กรอกไว้แม้ยังไม่ถูกต้อง เพื่อไม่ให้ช่องกระโดด) */
export interface ResizeChoice {
  preset: ResizePreset;
  /** ด้านยาวที่กำหนดเอง (px) */
  customPx: string;
  /** ขนาดไฟล์สูงสุด */
  maxValue: string;
  maxUnit: 'MB' | 'KB';
}

export const DEFAULT_RESIZE_CHOICE: ResizeChoice = { preset: 'original', customPx: '1280', maxValue: '1', maxUnit: 'MB' };

export const PRESETS: { value: ResizePreset; label: string }[] = [
  { value: 'original', label: 'ต้นฉบับ (คัดลอกไฟล์เดิม ไม่ย่อ)' },
  { value: '2048', label: 'ด้านยาว 2048 px — คมชัด เหมาะกับโพสต์และดูบนจอคอม' },
  { value: '1600', label: 'ด้านยาว 1600 px — เหมาะกับส่งทางแชต เช่น LINE' },
  { value: '1080', label: 'ด้านยาว 1080 px — ไฟล์เล็ก ดูบนมือถือ' },
  { value: 'custom', label: 'กำหนดด้านยาวเอง…' },
  { value: 'max', label: 'กำหนดขนาดไฟล์ไม่เกิน…' },
];

// ช่วงเดียวกับที่ Rust ตรวจ (export::Resize::validate) · KB/MB ฐาน 1024 ตรงกับ formatBytes
export const PX_MIN = 320;
export const PX_MAX = 10_000;
export const BYTES_MIN = 50 * 1024;
export const BYTES_MAX = 100 * 1024 * 1024;

/** แปลงตัวเลือกเป็นคำสั่งที่ส่งให้ Rust; ค่าที่กรอกไม่ถูกต้องคืนข้อความบอกผู้ใช้แทน */
export function toResize(choice: ResizeChoice): { resize: ExportResize } | { error: string } {
  switch (choice.preset) {
    case 'original':
      return { resize: { mode: 'original' } };
    case '2048':
    case '1600':
    case '1080':
      return { resize: { mode: 'long_edge', px: Number(choice.preset) } };
    case 'custom': {
      const px = Number(choice.customPx.trim());
      if (!Number.isInteger(px) || px < PX_MIN || px > PX_MAX) return { error: `ใส่ด้านยาวเป็นจำนวนเต็มระหว่าง ${PX_MIN} ถึง ${PX_MAX.toLocaleString('th-TH')} px` };
      return { resize: { mode: 'long_edge', px } };
    }
    case 'max': {
      const value = Number(choice.maxValue.trim().replace(',', '.'));
      const bytes = Math.round(value * (choice.maxUnit === 'MB' ? 1024 * 1024 : 1024));
      if (!choice.maxValue.trim() || !Number.isFinite(value) || bytes < BYTES_MIN || bytes > BYTES_MAX) {
        return { error: 'ใส่ขนาดไฟล์ระหว่าง 50 KB ถึง 100 MB' };
      }
      return { resize: { mode: 'max_bytes', bytes } };
    }
  }
}

/** อ่านค่าที่เคยเก็บไว้ ถ้าเสียหรือไม่ครบใช้ค่าเริ่มต้น */
export function parseResizeChoice(raw: string | null): ResizeChoice {
  if (!raw) return DEFAULT_RESIZE_CHOICE;
  try {
    const v = JSON.parse(raw) as Partial<ResizeChoice>;
    return {
      preset: PRESETS.some((p) => p.value === v.preset) ? (v.preset as ResizePreset) : DEFAULT_RESIZE_CHOICE.preset,
      customPx: typeof v.customPx === 'string' ? v.customPx : DEFAULT_RESIZE_CHOICE.customPx,
      maxValue: typeof v.maxValue === 'string' ? v.maxValue : DEFAULT_RESIZE_CHOICE.maxValue,
      maxUnit: v.maxUnit === 'KB' ? 'KB' : 'MB',
    };
  } catch {
    return DEFAULT_RESIZE_CHOICE;
  }
}
