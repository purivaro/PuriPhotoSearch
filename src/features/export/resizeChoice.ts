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

/** ตัวเลือกที่ใช้ได้และขั้นต่ำของแต่ละงาน (ส่งออกสำเนา / ย่อไฟล์ต้นฉบับ) */
export interface ResizeOptions {
  presets: { value: ResizePreset; label: string }[];
  pxMin: number;
  bytesMin: number;
}

export const EXPORT_OPTIONS: ResizeOptions = { presets: PRESETS, pxMin: PX_MIN, bytesMin: BYTES_MIN };

// ย่อไฟล์ต้นฉบับ: ไม่ต่ำกว่าภาพที่ตัววิเคราะห์ใบหน้าใช้ (Rust: shrink::MIN_EDGE / MIN_BYTES)
export const SHRINK_OPTIONS: ResizeOptions = {
  presets: [
    { value: '2048', label: 'ด้านยาว 2048 px — คมชัด ดูเต็มจอคอมได้ (แนะนำ)' },
    { value: '1600', label: 'ด้านยาว 1600 px — ประหยัดพื้นที่มากขึ้น' },
    { value: 'custom', label: 'กำหนดด้านยาวเอง (อย่างน้อย 1280 px)…' },
    { value: 'max', label: 'กำหนดขนาดไฟล์ไม่เกิน (อย่างน้อย 300 KB)…' },
  ],
  pxMin: 1280,
  bytesMin: 300 * 1024,
};

export const DEFAULT_SHRINK_CHOICE: ResizeChoice = { preset: '2048', customPx: '1600', maxValue: '1', maxUnit: 'MB' };

/** แปลงตัวเลือกเป็นคำสั่งที่ส่งให้ Rust; ค่าที่กรอกไม่ถูกต้องคืนข้อความบอกผู้ใช้แทน */
export function toResize(choice: ResizeChoice, options: ResizeOptions = EXPORT_OPTIONS): { resize: ExportResize } | { error: string } {
  if (!options.presets.some((p) => p.value === choice.preset)) return { error: 'เลือกขนาดที่จะย่อ' };
  switch (choice.preset) {
    case 'original':
      return { resize: { mode: 'original' } };
    case '2048':
    case '1600':
    case '1080':
      return { resize: { mode: 'long_edge', px: Number(choice.preset) } };
    case 'custom': {
      const px = Number(choice.customPx.trim());
      if (!Number.isInteger(px) || px < options.pxMin || px > PX_MAX) {
        return { error: `ใส่ด้านยาวเป็นจำนวนเต็มระหว่าง ${options.pxMin.toLocaleString('th-TH')} ถึง ${PX_MAX.toLocaleString('th-TH')} px` };
      }
      return { resize: { mode: 'long_edge', px } };
    }
    case 'max': {
      const value = Number(choice.maxValue.trim().replace(',', '.'));
      const bytes = Math.round(value * (choice.maxUnit === 'MB' ? 1024 * 1024 : 1024));
      if (!choice.maxValue.trim() || !Number.isFinite(value) || bytes < options.bytesMin || bytes > BYTES_MAX) {
        return { error: `ใส่ขนาดไฟล์ระหว่าง ${options.bytesMin / 1024} KB ถึง 100 MB` };
      }
      return { resize: { mode: 'max_bytes', bytes } };
    }
  }
}

/** อ่านค่าที่เคยเก็บไว้ ถ้าเสียหรือไม่ครบใช้ค่าเริ่มต้น */
export function parseResizeChoice(raw: string | null, fallback = DEFAULT_RESIZE_CHOICE, options = EXPORT_OPTIONS): ResizeChoice {
  if (!raw) return fallback;
  try {
    const v = JSON.parse(raw) as Partial<ResizeChoice>;
    return {
      preset: options.presets.some((p) => p.value === v.preset) ? (v.preset as ResizePreset) : fallback.preset,
      customPx: typeof v.customPx === 'string' ? v.customPx : fallback.customPx,
      maxValue: typeof v.maxValue === 'string' ? v.maxValue : fallback.maxValue,
      maxUnit: v.maxUnit === 'KB' ? 'KB' : 'MB',
    };
  } catch {
    return fallback;
  }
}
