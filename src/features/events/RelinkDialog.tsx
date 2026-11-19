import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { TriangleAlert } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { InlineNotice } from '@/components/ui/Feedback';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { RelinkEvent, RelinkPreview } from '@/lib/ipc/types';
import { invalidateCatalog } from '@/lib/queries';

function total(events: RelinkEvent[], key: 'photos' | 'matched'): number {
  return events.reduce((sum, e) => sum + e[key], 0);
}

/**
 * S14 (ขั้นยืนยัน) — ผลตรวจตำแหน่งใหม่ของโฟลเดอร์กิจกรรม: บอกว่าจับคู่รูปได้เท่าไร ด้วยหลักฐานอะไร
 * และอะไรจะยังหาไม่เจอ ก่อนเปลี่ยนตำแหน่งจริง
 */
export function RelinkDialog({ preview, onClose }: { preview: RelinkPreview | null; onClose: () => void }) {
  const qc = useQueryClient();
  const [includeSiblings, setIncludeSiblings] = useState(true);
  const commit = useMutation({
    mutationFn: (p: RelinkPreview) => call('commit_relink', { eventId: p.event.eventId, newFolder: p.event.newFolder, includeSiblings: includeSiblings && p.siblings.length > 0 }),
    onSuccess: (result) => {
      invalidateCatalog(qc);
      onClose();
      const unresolved = result.photosUnresolved > 0 ? ` อีก ${formatCount(result.photosUnresolved)} รูปยังหาไฟล์ไม่เจอ` : '';
      toast.success(`เชื่อมตำแหน่งใหม่แล้ว ${formatCount(result.photosRelinked)} รูปใน ${formatCount(result.events)} กิจกรรม${unresolved}`);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  const event = preview?.event;
  const nothing = event?.matched === 0;
  const unresolved = event ? event.missing + event.changed : 0;
  return (
    <Dialog
      open={preview !== null}
      onOpenChange={(open) => !open && !commit.isPending && onClose()}
      title="ใช้โฟลเดอร์นี้เป็นตำแหน่งใหม่?"
      size="md"
      dismissible={!commit.isPending}
      footer={
        <>
          <Button onClick={onClose} disabled={commit.isPending}>
            ยกเลิก
          </Button>
          <Button variant="primary" disabled={nothing} busy={commit.isPending} onClick={() => preview && commit.mutate(preview)} data-testid="relink-confirm">
            เชื่อมตำแหน่งใหม่
          </Button>
        </>
      }
    >
      {preview && event && (
        <div className="space-y-3 text-sm text-ink" data-testid="relink-preview">
          <dl className="divide-y divide-line rounded-xl border border-line">
            <div className="grid grid-cols-[110px_1fr] gap-3 px-3.5 py-2.5">
              <dt className="font-semibold">ตำแหน่งเดิม</dt>
              <dd className="min-w-0 break-all text-muted">{event.oldFolder}</dd>
            </div>
            <div className="grid grid-cols-[110px_1fr] gap-3 px-3.5 py-2.5">
              <dt className="font-semibold">ตำแหน่งใหม่</dt>
              <dd className="min-w-0 break-all">{event.newFolder}</dd>
            </div>
          </dl>
          {nothing ? (
            <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
              ไม่พบรูปของ “{event.title}” ในโฟลเดอร์นี้ จึงยังไม่เปลี่ยนตำแหน่ง ลองเลือกโฟลเดอร์ของกิจกรรมนี้อีกครั้ง
            </InlineNotice>
          ) : (
            <>
              <p data-testid="relink-matched">
                พบรูปของ “{event.title}” <span className="font-bold">{formatCount(event.matched)}</span> จาก {formatCount(event.photos)} รูป
                <span className="block text-[13px] text-muted">จับคู่จากตำแหน่งของไฟล์ในโฟลเดอร์และขนาดไฟล์ ไม่ได้เปรียบเทียบเนื้อไฟล์ทั้งไฟล์</span>
              </p>
              {unresolved > 0 && (
                <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
                  อีก {formatCount(unresolved)} รูปจะยังหาไฟล์ไม่เจอ
                  {event.missing > 0 && ` · ไม่พบไฟล์ ${formatCount(event.missing)}`}
                  {event.changed > 0 && ` · ไฟล์ถูกแก้หลังเพิ่มเข้าคลัง ${formatCount(event.changed)}`} — ชื่อบุคคลและข้อมูลของรูปเหล่านี้ยังอยู่ในคลัง
                </InlineNotice>
              )}
              {preview.siblings.length > 0 && (
                <label className="flex cursor-pointer items-start gap-2.5 rounded-xl border border-line px-3.5 py-2.5" data-testid="relink-siblings">
                  <input type="checkbox" className="mt-0.5 size-4 shrink-0 accent-[var(--action)]" checked={includeSiblings} onChange={(e) => setIncludeSiblings(e.target.checked)} />
                  <span>
                    เชื่อมอีก <span className="font-bold">{formatCount(preview.siblings.length)}</span> กิจกรรมจากโฟลเดอร์แม่เดียวกันด้วย ({formatCount(total(preview.siblings, 'matched'))} จาก{' '}
                    {formatCount(total(preview.siblings, 'photos'))} รูป)
                    <span className="block text-[13px] text-muted">กิจกรรมเหล่านี้หาโฟลเดอร์เดิมไม่เจอเหมือนกัน และพบที่ตำแหน่งใหม่แบบเดียวกัน</span>
                  </span>
                </label>
              )}
              {preview.siblingsNotFound > 0 && (
                <p className="text-[13px] text-muted">อีก {formatCount(preview.siblingsNotFound)} กิจกรรมจากโฟลเดอร์แม่เดียวกันยังหาไม่เจอที่ตำแหน่งใหม่ ต้องเชื่อมทีละกิจกรรม</p>
              )}
              <p className="text-[13px] text-muted">กิจกรรม ชื่อบุคคล และใบหน้าที่ยืนยันไว้ไม่เปลี่ยน ไฟล์รูปไม่ถูกแตะ</p>
            </>
          )}
        </div>
      )}
    </Dialog>
  );
}
