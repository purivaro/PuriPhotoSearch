import { SlidersHorizontal } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Select } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { formatCount } from '@/lib/format';
import type { EditFilter, FlagFilter } from '@/lib/ipc/types';
import type { PhotoResultsMeta } from './PhotoResults';

/** คำบอกตัวกรองการคัด/การแต่ง เช่น "ที่เลือกไว้และยังไม่แต่ง" ("" = ไม่ได้กรอง) */
export function progressPhrase(edit: EditFilter, flag: FlagFilter): string {
  const parts = [
    { any: '', picked: 'เลือกไว้', unflagged: 'ยังไม่คัด', rejected: 'ไม่เอา' }[flag],
    { any: '', unedited: 'ยังไม่แต่ง', edited: 'แต่งแล้ว' }[edit],
  ].filter(Boolean);
  return parts.length ? `ที่${parts.join('และ')}` : '';
}

/** ตัวกรองการแต่งในแอป (แต่งตามคน — DECISIONS ข้อ 32) */
export function EditFilterSelect({ value, onChange, className }: { value: EditFilter; onChange: (value: EditFilter) => void; className?: string }) {
  return (
    <Select value={value} onChange={(e) => onChange(e.target.value as EditFilter)} aria-label="กรองตามการแต่ง" className={className} data-testid="edit-filter">
      <option value="any">ทุกสถานะการแต่ง</option>
      <option value="unedited">ยังไม่แต่ง</option>
      <option value="edited">แต่งแล้ว</option>
    </Select>
  );
}

/**
 * ความคืบหน้า "แต่งแล้ว X จาก Y รูป" ของขอบเขตที่ดูอยู่ (นับก่อนกรองการแต่ง) + ปุ่มแต่งทีละรูปตามลำดับของผลนี้
 * — เปิดรูปแรกในหน้าแต่งภาพ แล้ว "บันทึก แล้วไปรูปถัดไป" ได้ทั้งชุด
 */
export function EditProgress({ meta, label = 'แต่งทีละรูป', className }: { meta: PhotoResultsMeta; label?: string; className?: string }) {
  const c = meta.coverage;
  const all = c?.eligiblePhotos ?? 0;
  const done = Math.min(c?.edited ?? 0, all);
  return (
    <span className={cn('flex flex-wrap items-center gap-x-3 gap-y-1.5', className)}>
      {all > 0 && (
        <span className="flex items-center gap-2 text-[13.5px] text-muted" data-testid="edit-progress">
          <span
            role="progressbar"
            aria-label="แต่งแล้ว"
            aria-valuemin={0}
            aria-valuemax={all}
            aria-valuenow={done}
            className="h-1.5 w-16 overflow-hidden rounded-full bg-sunken"
          >
            <span className="block h-full rounded-full bg-ok transition-[width]" style={{ width: `${(done / all) * 100}%` }} />
          </span>
          แต่งแล้ว {formatCount(done)} จาก {formatCount(all)} รูป
        </span>
      )}
      {meta.startEditing && (
        <Button size="sm" icon={<SlidersHorizontal className="size-4" aria-hidden />} onClick={meta.startEditing} data-testid="edit-in-order">
          {label}
        </Button>
      )}
    </span>
  );
}
