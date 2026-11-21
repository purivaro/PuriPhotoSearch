import { Smile } from 'lucide-react';
import { cn } from '@/lib/cn';
import { formatCount } from '@/lib/format';
import type { FaceBox, Retouch } from '@/lib/ipc/types';
import { SliderRow } from './AdjustControls';
import { NO_RETOUCH, RETOUCH_ITEMS, RETOUCH_LEVELS, faceBoxes, retouchIsActive, type RetouchKey } from './retouch';

interface RetouchPanelProps {
  retouch: Retouch | null;
  faces: FaceBox[];
  onChange: (retouch: Retouch | null) => void;
}

/** รีทัชใบหน้า (DECISIONS ข้อ 31): ผิวเนียน ลดริ้วรอย หน้าเรียว หน้าผอม — ทุกใบหน้าของรูป เลือกระดับหรือเลื่อนแถบละเอียด */
export function RetouchPanel({ retouch, faces, onChange }: RetouchPanelProps) {
  const current = retouch ?? NO_RETOUCH;
  // ใช้ใบหน้าของรูปตอนนี้ทุกครั้งที่ปรับ (ใบหน้าที่เพิ่ง/ลบในตัวดูภาพก็ตามทัน)
  const set = (key: RetouchKey, value: number) => {
    const next = { ...current, faces: faceBoxes(faces), [key]: value };
    onChange(retouchIsActive(next) ? next : null);
  };

  return (
    <section className="space-y-3" data-testid="retouch-panel">
      <h3 className="flex items-center gap-1.5 text-sm font-semibold text-ink">
        <Smile className="size-4 text-muted" aria-hidden /> รีทัชใบหน้า
        {faces.length > 0 && <span className="font-normal text-muted">· {formatCount(faces.length)} คน</span>}
      </h3>
      {faces.length === 0 ? (
        <p className="text-[12.5px] text-muted">ยังไม่มีใบหน้าในรูปนี้ — ให้แอปวิเคราะห์ใบหน้า หรือเพิ่มกรอบใบหน้าเองในตัวดูภาพก่อน</p>
      ) : (
        <>
          {RETOUCH_ITEMS.map(({ key, label, description }) => (
            <div key={key} className="space-y-1.5" data-testid={`retouch-${key}`}>
              <div className="flex items-center justify-between gap-2">
                <span className="text-[13px] font-semibold text-ink">{label}</span>
                <div className="flex shrink-0 gap-1" role="group" aria-label={`ระดับ${label}`}>
                  {RETOUCH_LEVELS.map((level) => (
                    <button
                      key={level.label}
                      type="button"
                      onClick={() => set(key, level.value)}
                      aria-pressed={current[key] === level.value}
                      className={cn(
                        'h-7 rounded-full border px-2.5 text-[12px] font-semibold transition-colors',
                        current[key] === level.value ? 'border-action bg-action text-white' : 'border-line-strong bg-surface text-ink hover:border-action hover:text-action',
                      )}
                      data-testid={`retouch-${key}-level`}
                    >
                      {level.label}
                    </button>
                  ))}
                </div>
              </div>
              <p className="text-[12px] text-muted">{description}</p>
              <SliderRow
                id={`retouch-${key}-slider`}
                label="ปรับละเอียด"
                value={current[key]}
                min={0}
                max={100}
                step={1}
                defaultValue={0}
                format={(v) => formatCount(v)}
                onChange={(v) => set(key, v)}
              />
            </div>
          ))}
          <p className="text-[12px] text-muted">ใช้กับทุกใบหน้าในรูป · หน้าเรียว/หน้าผอมดัดเฉพาะขอบหน้า ตา จมูก ปากอยู่ที่เดิม</p>
        </>
      )}
    </section>
  );
}
