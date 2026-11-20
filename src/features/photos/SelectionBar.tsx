import type { ReactNode } from 'react';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ClipboardPaste, Minimize2, Upload, X } from 'lucide-react';
import { useEditClipboard } from '@/stores/editClipboard';
import { Button } from '@/components/ui/Button';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { Id } from '@/lib/ipc/types';
import type { PhotoQuery } from '@/lib/queries';
import { EMPTY_SELECTION, selectLoaded, selectSnapshot, selectionCount, type Selection } from './selection';

interface SelectionBarProps {
  selection: Selection;
  onSelectionChange: (selection: Selection) => void;
  loadedIds: Id[];
  /** จำนวนผลทั้งหมดของเงื่อนไขปัจจุบัน */
  total: number;
  /** เงื่อนไขที่ใช้สร้าง snapshot ของ "ผลทั้งหมด" */
  query: PhotoQuery;
  onExport: () => void;
  /** ย่อไฟล์ต้นฉบับในที่เดิม */
  onShrink: () => void;
  /** วางการแต่งที่คัดลอกไว้ */
  onPasteEdit: () => void;
  /** ปุ่มเฉพาะหน้า วางก่อนปุ่มส่งออก */
  actions?: ReactNode;
}

/** แถบเลือกหลายรูป: บอกจำนวนและขอบเขตตลอดเวลา อยู่ท้าย grid โดยไม่บังแถวสุดท้าย */
export function SelectionBar({ selection, onSelectionChange, loadedIds, total, query, onExport, onShrink, onPasteEdit, actions }: SelectionBarProps) {
  const count = selectionCount(selection);
  const copied = useEditClipboard((s) => s.copied);
  const snapshot = useMutation({
    mutationFn: () => call('create_selection', query),
    onSuccess: (result) => {
      onSelectionChange(selectSnapshot(result.selectionToken, result.count));
      toast.success(`เลือกผลทั้งหมด ${formatCount(result.count)} รูป ณ ตอนนี้แล้ว รูปที่เพิ่มเข้ามาภายหลังจะไม่ถูกรวม`);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  if (count === 0) return null;
  const isSnapshot = selection.kind === 'snapshot';
  const allLoadedSelected = !isSnapshot && loadedIds.length > 0 && loadedIds.every((id) => selection.ids.has(id));

  return (
    <div className="sticky bottom-4 z-10 mt-4" data-testid="selection-bar">
      <div role="region" aria-label="รูปที่เลือก" className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-2xl border border-brand-line bg-surface px-4 py-3 shadow-pop">
        <div className="min-w-0 flex-1">
          <p className="text-[15px] font-bold text-ink" aria-live="polite" data-testid="selection-count">
            เลือก {formatCount(count)} รูป
          </p>
          <p className="text-[13px] text-muted">
            {isSnapshot
              ? `ผลค้นหาทั้งหมด ณ ตอนที่เลือก${selection.excluded.size > 0 ? ` (เอาออกเอง ${formatCount(selection.excluded.size)} รูป)` : ''}`
              : `จากรูปที่ติ๊กเลือกเอง · แสดงอยู่ ${formatCount(loadedIds.length)} จาก ${formatCount(total)} รูป`}
          </p>
        </div>
        {!isSnapshot && !allLoadedSelected && (
          <Button size="sm" variant="ghost" onClick={() => onSelectionChange(selectLoaded(selection, loadedIds))}>
            เลือกทั้งหมดที่แสดง ({formatCount(loadedIds.length)})
          </Button>
        )}
        {!isSnapshot && total > 0 && (
          <Button size="sm" variant="ghost" busy={snapshot.isPending} onClick={() => snapshot.mutate()} data-testid="select-all-results">
            เลือกผลทั้งหมด {formatCount(total)} รูป
          </Button>
        )}
        <Button size="sm" icon={<X className="size-4" aria-hidden />} onClick={() => onSelectionChange(EMPTY_SELECTION)} data-testid="selection-clear">
          ล้างที่เลือก
        </Button>
        {actions}
        {copied && (
          <Button icon={<ClipboardPaste className="size-4" aria-hidden />} onClick={onPasteEdit} data-testid="paste-edit-button" title={`วางการแต่งจาก ${copied.filename}`}>
            วางการแต่ง
          </Button>
        )}
        <Button icon={<Minimize2 className="size-4" aria-hidden />} onClick={onShrink} data-testid="shrink-button">
          ย่อไฟล์ต้นฉบับ
        </Button>
        <Button variant="primary" icon={<Upload className="size-4" aria-hidden />} onClick={onExport} data-testid="export-button">
          ส่งออกสำเนา
        </Button>
      </div>
    </div>
  );
}
