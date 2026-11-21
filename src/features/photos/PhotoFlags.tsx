import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Ban, Flag } from 'lucide-react';
import { Select } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { AiFlag, Coverage, FlagFilter, PhotoFlag, SelectionSpec } from '@/lib/ipc/types';

/** คัดก่อนแต่ง (DECISIONS ข้อ 33): ชื่อของการคัด และคีย์ลัดแบบ Lightroom (ใช้ event.code จึงกดได้แม้แป้นเป็นภาษาไทย) */
export const FLAG_KEYS: Record<string, PhotoFlag | null> = { KeyP: 'pick', KeyX: 'reject', KeyU: null };
export const FLAG_LABEL: Record<PhotoFlag, string> = { pick: 'เลือก', reject: 'ไม่เอา' };

/** คีย์ลัดของการคัด ถ้าเป็น (undefined = ไม่ใช่คีย์ของการคัด) */
export function flagForKey(event: KeyboardEvent | React.KeyboardEvent): PhotoFlag | null | undefined {
  if (event.metaKey || event.ctrlKey || event.altKey) return undefined;
  return event.code in FLAG_KEYS ? FLAG_KEYS[event.code] : undefined;
}

/** ตั้งการคัดให้รูปที่เลือก แล้วให้รายการรูปและรายละเอียดรูปโหลดใหม่ */
export function useSetPhotoFlags() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ selection, flag }: { selection: SelectionSpec; flag: PhotoFlag | null }) => call('set_photo_flags', { selection, flag }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['photos'] });
      void qc.invalidateQueries({ queryKey: ['photo'] });
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

export function FlagFilterSelect({ value, onChange, className }: { value: FlagFilter; onChange: (value: FlagFilter) => void; className?: string }) {
  return (
    <Select value={value} onChange={(e) => onChange(e.target.value as FlagFilter)} aria-label="กรองตามการคัด" className={className} data-testid="flag-filter">
      <option value="any">ทุกการคัด</option>
      <option value="picked">เลือกไว้</option>
      <option value="unflagged">ยังไม่คัด</option>
      <option value="rejected">ไม่เอา</option>
    </Select>
  );
}

/** เหตุผลที่ AI คัดรูปนี้ เป็นภาษาผู้ใช้ (DECISIONS ข้อ 37) */
export function aiFlagReason(ai: AiFlag): string {
  if (ai.reason === 'blur') return 'เบลอกว่ารูปอื่นในงานนี้มาก';
  if (ai.reason === 'duplicate') return 'ถ่ายซ้ำกับรูปที่เลือกไว้ของช่วงเดียวกัน';
  return 'ดีที่สุดของช่วงที่ถ่ายต่อเนื่อง';
}

/** ป้ายการคัดบนภาพย่อ: เลือก = ธงสีเขียว · ไม่เอา = กากบาท (ภาพย่อหรี่ลงด้วย) · AI คัด = มีคำว่า AI กำกับ */
export function FlagBadge({ flag, compact, ai }: { flag: PhotoFlag; compact?: boolean; ai?: AiFlag }) {
  const Icon = flag === 'pick' ? Flag : Ban;
  const label = ai ? `AI ${FLAG_LABEL[flag]}` : FLAG_LABEL[flag];
  const hint = flag === 'pick' ? 'เลือกไว้ (กด P ในตัวดูภาพ)' : 'ไม่เอา — ไฟล์ไม่ถูกลบ (กด X ในตัวดูภาพ)';
  return (
    <span
      title={ai ? `AI ${FLAG_LABEL[flag]}: ${aiFlagReason(ai)} — กด P / X / U ในตัวดูภาพเพื่อเปลี่ยนเอง` : hint}
      data-testid="photo-card-flag"
      data-flag={flag}
      data-ai={ai ? ai.reason : undefined}
      className={cn(
        'inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-semibold text-white',
        flag === 'pick' ? 'bg-ok' : 'bg-black/65',
      )}
    >
      <Icon className="size-3 shrink-0" fill={flag === 'pick' ? 'currentColor' : 'none'} aria-hidden />
      {ai && <span className="text-[10px] font-bold tracking-wide">AI</span>}
      {compact ? <span className="sr-only">{label}</span> : FLAG_LABEL[flag]}
    </span>
  );
}

/** "เลือกไว้ X · ไม่เอา Y" ของขอบเขตที่ดูอยู่ (นับก่อนกรอง) — แสดงเมื่อเริ่มคัดแล้ว */
export function FlagSummary({ coverage }: { coverage: Coverage | null }) {
  if (!coverage || coverage.picked + coverage.rejected === 0) return null;
  const left = coverage.eligiblePhotos - coverage.picked - coverage.rejected;
  return (
    <span className="flex items-center gap-2.5 text-[13.5px] text-muted" data-testid="flag-summary">
      <span className="inline-flex items-center gap-1">
        <Flag className="size-3.5 text-ok" fill="currentColor" aria-hidden />
        เลือกไว้ {formatCount(coverage.picked)}
      </span>
      <span className="inline-flex items-center gap-1">
        <Ban className="size-3.5" aria-hidden />
        ไม่เอา {formatCount(coverage.rejected)}
      </span>
      {left > 0 && <span>ยังไม่คัด {formatCount(left)}</span>}
    </span>
  );
}
