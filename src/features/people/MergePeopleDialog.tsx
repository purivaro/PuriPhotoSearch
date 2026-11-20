import { useRef, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Merge, TriangleAlert, UserPlus, X } from 'lucide-react';
import { Button, IconButton } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { errorMessage, formatCount, newRequestId } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { PersonSummary } from '@/lib/ipc/types';
import { invalidatePeopleData } from '@/lib/queries';
import { FaceAvatar } from './FaceAvatar';
import { PersonPicker } from './PersonPicker';

interface MergePeopleDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** คนที่เหลืออยู่หลังรวม */
  target: PersonSummary;
  /** คนที่เลือกไว้ให้ตั้งแต่เปิด (เช่น คนชื่อเดียวกัน) */
  initial?: PersonSummary[];
}

/** S17 — รวมบุคคลที่ซ้ำเข้ากับคนที่เปิดอยู่: เลือกคน ดูว่าจะย้ายอะไร แล้วยืนยัน (ย้อนกลับไม่ได้) */
export function MergePeopleDialog({ open, onOpenChange, target, initial = [] }: MergePeopleDialogProps) {
  const qc = useQueryClient();
  const [chosen, setChosen] = useState<PersonSummary[]>(initial);
  const [picking, setPicking] = useState(initial.length === 0);
  const [lastOpen, setLastOpen] = useState(open);
  // เปิดใหม่ทุกครั้งเริ่มจากคนที่เสนอไว้
  if (open !== lastOpen) {
    setLastOpen(open);
    if (open) {
      setChosen(initial);
      setPicking(initial.length === 0);
    }
  }
  const requestId = useRef<string | null>(null);
  const merge = useMutation({
    mutationFn: () => {
      requestId.current ??= newRequestId();
      return call('merge_persons', { targetId: target.id, sourceIds: chosen.map((p) => p.id), requestId: requestId.current });
    },
    onSuccess: (result) => {
      requestId.current = null;
      invalidatePeopleData(qc);
      toast.success(`รวม ${formatCount(result.merged)} คนเข้ากับ ${target.displayName} แล้ว`);
      onOpenChange(false);
    },
  });

  const photos = (p: PersonSummary) => p.confirmedPhotoCount + p.aiPhotoCount;
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !merge.isPending && onOpenChange(next)}
      dismissible={!merge.isPending}
      size="md"
      title={`รวมบุคคลเข้ากับ ${target.displayName}`}
      description="ใช้เมื่อคนเดียวกันถูกเพิ่มไว้หลายครั้ง คนที่เลือกจะหายจากรายชื่อ และข้อมูลทั้งหมดของเขามาอยู่ที่คนนี้"
      footer={
        <>
          <Button variant="secondary" onClick={() => onOpenChange(false)} disabled={merge.isPending}>
            ยกเลิก
          </Button>
          <Button
            variant="primary"
            icon={<Merge className="size-[18px]" aria-hidden />}
            disabled={chosen.length === 0}
            busy={merge.isPending}
            onClick={() => merge.mutate()}
            data-testid="merge-confirm"
          >
            {chosen.length ? `รวม ${formatCount(chosen.length)} คน` : 'รวม'}
          </Button>
        </>
      }
    >
      <div className="space-y-4" data-testid="merge-dialog">
        <div className="flex items-center gap-3 rounded-xl border border-brand-line bg-brand-soft px-3.5 py-3">
          <FaceAvatar asset={target.coverAsset} name={target.displayName} className="size-11" />
          <div className="min-w-0 flex-1">
            <p className="text-[13px] font-semibold text-action">คนที่เก็บไว้</p>
            <p className="truncate text-[15px] font-bold text-ink">{target.displayName}</p>
          </div>
          <p className="shrink-0 text-sm text-muted">{formatCount(photos(target))} รูป</p>
        </div>

        <div>
          <p className="mb-2 text-sm font-semibold text-ink">คนที่จะรวมเข้ามา</p>
          {chosen.length > 0 && (
            <ul className="mb-2.5 space-y-2" data-testid="merge-chosen">
              {chosen.map((p) => (
                <li key={p.id} className="flex items-center gap-3 rounded-xl border border-line bg-surface px-3.5 py-2.5">
                  <FaceAvatar asset={p.coverAsset} name={p.displayName} className="size-10" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[15px] font-semibold text-ink">{p.displayName}</p>
                    <p className="text-[13px] text-muted">{formatCount(photos(p))} รูป</p>
                  </div>
                  <IconButton label={`ไม่รวม ${p.displayName}`} size="sm" onClick={() => setChosen((list) => list.filter((x) => x.id !== p.id))} disabled={merge.isPending}>
                    <X className="size-4" aria-hidden />
                  </IconButton>
                </li>
              ))}
            </ul>
          )}
          {picking ? (
            <div className="overflow-hidden rounded-xl border border-line-strong bg-surface">
              <PersonPicker
                className="w-full max-w-none"
                allowCreate={false}
                busy={merge.isPending}
                excludeIds={[target.id, ...chosen.map((p) => p.id)]}
                placeholder="พิมพ์ชื่อคนที่ซ้ำเพื่อเพิ่ม"
                onPick={(choice) => {
                  if (choice.kind !== 'existing') return;
                  setChosen((list) => [...list, choice.person]);
                  setPicking(false);
                }}
              />
            </div>
          ) : (
            <Button size="sm" icon={<UserPlus className="size-4" aria-hidden />} onClick={() => setPicking(true)} disabled={merge.isPending} data-testid="merge-add">
              เพิ่มคนอื่นที่ซ้ำ
            </Button>
          )}
        </div>

        <ul className="list-disc space-y-1 pl-5 text-[13px] text-muted">
          <li>ย้ายมา: รูปที่ยืนยันแล้ว ใบหน้าตัวอย่าง รูปที่แจ้งว่า “ไม่ใช่” กลุ่ม ชื่ออื่น และการปักหมุด</li>
          <li>ชื่อของคนที่ถูกรวม ถ้าต่างจาก “{target.displayName}” จะเก็บเป็นชื่ออื่น ค้นด้วยชื่อนั้นได้ต่อ</li>
          <li>รูปต้นฉบับไม่ถูกแตะ</li>
        </ul>
        <p className="flex items-start gap-2 text-[13px] font-semibold text-warn">
          <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
          รวมแล้วแยกกลับไม่ได้ ตรวจให้แน่ใจว่าเป็นคนเดียวกัน
        </p>
        {merge.isError && (
          <p role="alert" className="text-sm text-danger" data-testid="merge-failed">
            รวมไม่สำเร็จ: {errorMessage(merge.error)} — ข้อมูลยังเหมือนเดิม กดรวมเพื่อลองใหม่ได้
          </p>
        )}
      </div>
    </Dialog>
  );
}
