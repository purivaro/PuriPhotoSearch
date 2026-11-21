import type { ReactNode } from 'react';
import type { Adjust, SliderKey } from './adjust';
import { SliderRow } from './AdjustControls';

/** แถบเลื่อนของค่าแสงและสี — ใช้ทั้งการปรับทั้งภาพและการปรับเฉพาะส่วน */
export interface SliderSpec {
  key: SliderKey;
  label: string;
  min: number;
  max: number;
  step: number;
  /** ข้อความปลายสองข้าง (ซ้าย, ขวา) */
  ends?: [string, string];
  format?: (v: number) => string;
}

export const LIGHT: SliderSpec[] = [
  { key: 'exposure', label: 'ค่าแสง', min: -3, max: 3, step: 0.05, format: (v) => `${v > 0 ? '+' : ''}${v.toFixed(2)} EV` },
  { key: 'contrast', label: 'คอนทราสต์', min: -100, max: 100, step: 1 },
  { key: 'highlights', label: 'ส่วนสว่าง', min: -100, max: 100, step: 1 },
  { key: 'shadows', label: 'เงา', min: -100, max: 100, step: 1 },
  { key: 'whites', label: 'จุดขาว', min: -100, max: 100, step: 1 },
  { key: 'blacks', label: 'จุดดำ', min: -100, max: 100, step: 1 },
];

// ปรับที่ดูพิกเซลรอบข้าง (DECISIONS ข้อ 27) — ชื่อเดียวกับ Lightroom ในวงเล็บ
export const PRESENCE: SliderSpec[] = [
  { key: 'texture', label: 'พื้นผิว (Texture)', min: -100, max: 100, step: 1, ends: ['นุ่ม เนียน', 'คม'] },
  { key: 'clarity', label: 'ความชัด (Clarity)', min: -100, max: 100, step: 1, ends: ['นวล', 'มีมิติ'] },
  { key: 'dehaze', label: 'ลดหมอก (Dehaze)', min: -100, max: 100, step: 1, ends: ['เติมหมอก', 'ใส'] },
];

export const COLOR: SliderSpec[] = [
  { key: 'temperature', label: 'อุณหภูมิสี', min: -100, max: 100, step: 1, ends: ['เย็น', 'อุ่น'] },
  { key: 'tint', label: 'โทนสี', min: -100, max: 100, step: 1, ends: ['เขียว', 'ม่วงแดง'] },
  { key: 'vibrance', label: 'ความสดของสี', min: -100, max: 100, step: 1 },
  { key: 'saturation', label: 'ความอิ่มสี', min: -100, max: 100, step: 1 },
];

export function SliderGroup({
  title,
  icon,
  specs,
  adjust,
  onChange,
  action,
  idPrefix = 'editor',
}: {
  title: string;
  icon?: ReactNode;
  specs: SliderSpec[];
  adjust: Adjust;
  onChange: (patch: Partial<Adjust>) => void;
  /** ปุ่มเสริมท้ายหัวข้อ */
  action?: ReactNode;
  /** คำนำหน้า id ของแถบ (หลายชุดในหน้าเดียวกันต้องไม่ซ้ำ) */
  idPrefix?: string;
}) {
  return (
    <section>
      <div className="mb-2 flex min-h-8 items-center justify-between gap-2">
        <h3 className="flex items-center gap-1.5 text-sm font-semibold text-ink">
          {icon}
          {title}
        </h3>
        {action}
      </div>
      <div className="space-y-3">
        {specs.map((spec) => (
          <SliderRow
            key={spec.key}
            id={`${idPrefix}-${spec.key}`}
            label={spec.label}
            value={adjust[spec.key]}
            min={spec.min}
            max={spec.max}
            step={spec.step}
            ends={spec.ends}
            format={spec.format}
            onChange={(v) => onChange({ [spec.key]: v })}
          />
        ))}
      </div>
    </section>
  );
}

