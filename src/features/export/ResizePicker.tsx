import { useId } from 'react';
import { Select, TextInput } from '@/components/ui/Form';
import { EXPORT_OPTIONS, type ResizeChoice, type ResizeOptions, type ResizePreset } from './resizeChoice';

export type ResizeHints = Record<'original' | 'long_edge' | 'max', string>;

const EXPORT_HINTS: ResizeHints = {
  original: 'คัดลอกไฟล์ต้นฉบับทุกไบต์ คมชัดที่สุด ขนาดไฟล์เท่าต้นฉบับ',
  long_edge: 'ย่อเฉพาะรูปที่ใหญ่กว่านี้ (รูปที่เล็กกว่าคัดลอกไฟล์เดิม) · บันทึกเป็น JPEG คุณภาพสูง เก็บวันที่ถ่ายและข้อมูลกล้องไว้',
  max: 'ย่อเฉพาะรูปที่ไฟล์ใหญ่กว่านี้ โดยลดขนาดภาพก่อนแล้วใช้คุณภาพสูงสุดที่ยังไม่เกิน · บันทึกเป็น JPEG เก็บวันที่ถ่ายไว้',
};

interface ResizePickerProps {
  value: ResizeChoice;
  onChange: (value: ResizeChoice) => void;
  error?: string | null;
  /** ตัวเลือกของงานนี้ (ค่าเริ่มต้น = ส่งออกสำเนา) */
  options?: ResizeOptions;
  hints?: ResizeHints;
  label?: string;
}

/** เลือกขนาด: ต้นฉบับ / ด้านยาวสำเร็จรูป / ด้านยาวที่กำหนดเอง / ขนาดไฟล์ไม่เกิน — ใช้ทั้งส่งออกสำเนาและย่อไฟล์ต้นฉบับ */
export function ResizePicker({ value, onChange, error, options = EXPORT_OPTIONS, hints = EXPORT_HINTS, label = 'ขนาดของสำเนา' }: ResizePickerProps) {
  const id = useId();
  const kind = value.preset === 'original' ? 'original' : value.preset === 'max' ? 'max' : 'long_edge';
  return (
    <div data-testid="export-resize">
      <label htmlFor={id} className="mb-1.5 block text-sm font-semibold text-ink">
        {label}
      </label>
      <div className="flex flex-wrap items-center gap-2">
        <Select
          id={id}
          value={value.preset}
          onChange={(e) => onChange({ ...value, preset: e.target.value as ResizePreset })}
          className="max-w-full min-w-0"
          aria-describedby={`${id}-hint`}
          data-testid="export-resize-preset"
        >
          {options.presets.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </Select>
        {/* ช่องกรอกอยู่ใน span ที่กำหนดความกว้าง: ตัวควบคุมของฟอร์มกว้างเต็มพื้นที่เสมอ (cn ไม่แทนคลาส w-full) */}
        {value.preset === 'custom' && (
          <span className="flex shrink-0 items-center gap-1.5 text-sm whitespace-nowrap text-muted">
            <span className="w-24">
              <TextInput
                inputMode="numeric"
                value={value.customPx}
                onChange={(e) => onChange({ ...value, customPx: e.target.value })}
                aria-label="ด้านยาว (พิกเซล)"
                invalid={Boolean(error)}
                data-testid="export-resize-px"
              />
            </span>
            px
          </span>
        )}
        {value.preset === 'max' && (
          <span className="flex shrink-0 items-center gap-1.5 text-sm whitespace-nowrap text-muted">
            <span className="w-20">
              <TextInput
                inputMode="decimal"
                value={value.maxValue}
                onChange={(e) => onChange({ ...value, maxValue: e.target.value })}
                aria-label="ขนาดไฟล์สูงสุด"
                invalid={Boolean(error)}
                data-testid="export-resize-max"
              />
            </span>
            <Select
              className="w-[5.5rem]"
              value={value.maxUnit}
              onChange={(e) => onChange({ ...value, maxUnit: e.target.value as ResizeChoice['maxUnit'] })}
              aria-label="หน่วยขนาดไฟล์"
            >
              <option value="MB">MB</option>
              <option value="KB">KB</option>
            </Select>
            ต่อรูป
          </span>
        )}
      </div>
      {error ? (
        <p id={`${id}-hint`} role="alert" className="mt-1.5 text-[13px] text-danger">
          {error}
        </p>
      ) : (
        <p id={`${id}-hint`} className="mt-1.5 text-[13px] text-muted">
          {hints[kind]}
        </p>
      )}
    </div>
  );
}
