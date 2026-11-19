import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ArchiveRestore, DatabaseBackup, FolderOpen, RotateCcw, Save, TriangleAlert } from 'lucide-react';
import { useFolderChooser } from '@/app/FolderChooser';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { ErrorState, InlineNotice, LoadingBlock } from '@/components/ui/Feedback';
import { errorMessage, formatBytes, formatCount, formatInstant } from '@/lib/format';
import { call, isTauri, restartApp } from '@/lib/ipc/client';
import type { BackupCounts, BackupInfo, BackupManifest, RestorePreview } from '@/lib/ipc/types';

const CARD = 'rounded-card border border-line bg-surface p-6 shadow-card';
const BACKUP_KEY = ['backup', 'status'] as const;

const KIND_LABEL: Record<BackupManifest['kind'], string> = {
  manual: 'สำรองเอง',
  auto: 'สำรองอัตโนมัติ',
  before_restore: 'คลังก่อนกู้คืน',
  before_upgrade: 'คลังก่อนอัปเกรดแอป',
};

function countsText(counts: BackupCounts): string {
  return `${formatCount(counts.events)} กิจกรรม · ${formatCount(counts.photos)} รูป · ${formatCount(counts.persons)} บุคคล · ${formatCount(counts.confirmations)} ใบหน้าที่ยืนยัน`;
}

/** S20 — สำรองและกู้คืนคลัง: บอกเสมอว่าชุดสำรองไม่รวมไฟล์รูปต้นฉบับ และการกู้คืนเก็บคลังปัจจุบันไว้ก่อน */
export function BackupSection() {
  const qc = useQueryClient();
  const choose = useFolderChooser();
  const status = useQuery({ queryKey: BACKUP_KEY, queryFn: () => call('get_backup_status', {}) });
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const refresh = () => void qc.invalidateQueries({ queryKey: BACKUP_KEY });
  const onError = (error: unknown) => toast.error(errorMessage(error));

  const backup = useMutation({
    mutationFn: (destination: string) => call('create_backup', { destination }),
    onSuccess: (info) => {
      refresh();
      toast.success(`สำรองคลังแล้ว ขนาด ${formatBytes(info.bytes)}`);
    },
    onError,
  });
  const inspect = useMutation({
    mutationFn: (path: string) => call('preview_restore', { path }),
    onSuccess: setPreview,
    onError,
  });
  const stage = useMutation({
    mutationFn: (path: string) => call('stage_restore', { path }),
    onSuccess: () => {
      setPreview(null);
      refresh();
      toast.success('เตรียมกู้คืนแล้ว คลังที่กู้คืนจะถูกใช้เมื่อเปิดแอปครั้งถัดไป');
    },
    onError,
  });
  const cancel = useMutation({
    mutationFn: () => call('cancel_restore', {}),
    onSuccess: () => {
      refresh();
      toast.success('ยกเลิกการกู้คืนแล้ว คลังปัจจุบันไม่เปลี่ยน');
    },
    onError,
  });

  const backupNow = async () => {
    const destination = await choose({ title: 'เลือกโฟลเดอร์เก็บชุดสำรอง', hint: 'แนะนำให้เก็บไว้คนละไดรฟ์กับเครื่องนี้ เช่น ไดรฟ์ภายนอก' });
    if (destination) backup.mutate(destination);
  };
  const restoreFromFolder = async () => {
    const path = await choose({ title: 'เลือกโฟลเดอร์ชุดสำรอง', hint: 'โฟลเดอร์ที่ชื่อขึ้นต้นด้วย PuriPhotoSearch-backup- และมีไฟล์ manifest.json' });
    if (path) inspect.mutate(path);
  };

  if (status.isPending) return <LoadingBlock label="กำลังโหลดข้อมูลชุดสำรอง…" />;
  if (status.isError) return <ErrorState compact error={status.error} title="โหลดข้อมูลชุดสำรองไม่สำเร็จ" onRetry={() => void status.refetch()} />;
  const data = status.data;
  const pending = data.pendingRestore;

  return (
    <>
      {pending && (
        <div data-testid="restore-pending">
        <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
          <p className="font-semibold">รอกู้คืน: คลังจากชุดสำรองวันที่ {formatInstant(pending.createdAt)} จะถูกใช้เมื่อเปิดแอปครั้งถัดไป</p>
          <p>
            {countsText(pending.counts)} — คลังปัจจุบันจะถูกเก็บเป็นชุดสำรองก่อนถูกแทน
            {!isTauri && ' (ปิดแล้วเปิดแอปใหม่เพื่อเริ่มใช้)'}
          </p>
          <div className="mt-2.5 flex flex-wrap gap-2">
            {isTauri && (
              <Button size="sm" variant="primary" icon={<RotateCcw className="size-4" aria-hidden />} onClick={() => void restartApp().catch(onError)} data-testid="restore-restart">
                เปิดแอปใหม่ตอนนี้
              </Button>
            )}
            <Button size="sm" busy={cancel.isPending} onClick={() => cancel.mutate()} data-testid="restore-cancel">
              ยกเลิกการกู้คืน
            </Button>
          </div>
        </InlineNotice>
        </div>
      )}

      <section className={CARD} data-testid="backup-card">
        <h2 className="flex items-center gap-2 text-lg font-bold text-ink">
          <DatabaseBackup className="size-5 text-muted" aria-hidden /> สำรองคลัง
        </h2>
        <p className="mt-2 text-sm text-ink">
          ชุดสำรองเก็บชื่อบุคคล ใบหน้าที่ยืนยัน กิจกรรม และข้อมูลใบหน้าที่วิเคราะห์แล้ว <span className="font-semibold">ไม่รวมไฟล์รูปต้นฉบับ</span> ภาพย่อ
          และตัววิเคราะห์ใบหน้า (สองอย่างหลังสร้างใหม่หรือดาวน์โหลดใหม่ได้)
        </p>
        <p className="mt-2 text-sm text-muted" data-testid="backup-last">
          {data.lastBackupAt ? (
            <>
              สำรองเองล่าสุด {formatInstant(data.lastBackupAt)} ที่{' '}
              <span className="break-all select-text" data-selectable>
                {data.lastBackupPath}
              </span>
            </>
          ) : (
            'ยังไม่เคยสำรองไปที่อื่น — ชุดอัตโนมัติด้านล่างอยู่ในเครื่องเดียวกับคลัง ถ้าดิสก์เสียจะหายไปด้วยกัน'
          )}
        </p>
        <Button variant="primary" className="mt-4" icon={<Save className="size-[18px]" aria-hidden />} busy={backup.isPending} onClick={() => void backupNow()} data-testid="backup-now">
          สำรองไปยังโฟลเดอร์…
        </Button>
      </section>

      <section className={CARD} data-testid="restore-card">
        <h2 className="flex items-center gap-2 text-lg font-bold text-ink">
          <ArchiveRestore className="size-5 text-muted" aria-hidden /> กู้คืนคลัง
        </h2>
        <p className="mt-2 text-sm text-ink">
          แอปสำรองคลังให้เองวันละครั้ง (เก็บ 3 ชุดล่าสุด) และเก็บคลังไว้ทุกครั้งก่อนกู้คืนหรืออัปเกรด การกู้คืนจะใช้ได้เมื่อเปิดแอปใหม่
          และคลังที่ถูกแทนจะถูกเก็บไว้ให้กู้กลับได้เสมอ
        </p>
        {data.automatic.length === 0 ? (
          <p className="mt-3 rounded-xl border border-dashed border-line-strong px-4 py-3 text-sm text-muted" data-testid="backup-empty">
            ยังไม่มีชุดที่แอปเก็บไว้ ชุดแรกจะถูกสร้างเมื่อเปิดแอปครั้งถัดไปหลังคลังมีข้อมูล
          </p>
        ) : (
          <ul className="mt-3 divide-y divide-line rounded-xl border border-line" data-testid="backup-list">
            {data.automatic.map((set) => (
              <BackupRow key={set.path} set={set} busy={inspect.isPending && inspect.variables === set.path} onRestore={() => inspect.mutate(set.path)} />
            ))}
          </ul>
        )}
        <Button className="mt-4" icon={<FolderOpen className="size-[18px]" aria-hidden />} busy={inspect.isPending && !data.automatic.some((s) => s.path === inspect.variables)} onClick={() => void restoreFromFolder()} data-testid="restore-from-folder">
          กู้คืนจากชุดสำรองในโฟลเดอร์อื่น…
        </Button>
        <p className="mt-3 text-[13px] text-muted">
          ที่เก็บชุดของแอป:{' '}
          <span className="break-all select-text" data-selectable>
            {data.backupsDir}
          </span>
        </p>
      </section>

      <Dialog
        open={preview !== null}
        onOpenChange={(open) => !open && !stage.isPending && setPreview(null)}
        title="กู้คืนคลังจากชุดสำรองนี้?"
        size="md"
        dismissible={!stage.isPending}
        footer={
          <>
            <Button onClick={() => setPreview(null)} disabled={stage.isPending}>
              ยกเลิก
            </Button>
            <Button variant="primary" busy={stage.isPending} onClick={() => preview && stage.mutate(preview.backup.path)} data-testid="restore-confirm">
              กู้คืนเมื่อเปิดแอปครั้งถัดไป
            </Button>
          </>
        }
      >
        {preview && (
          <div className="space-y-3 text-sm text-ink" data-testid="restore-preview">
            <dl className="divide-y divide-line rounded-xl border border-line">
              <div className="grid grid-cols-[150px_1fr] gap-3 px-3.5 py-2.5">
                <dt className="font-semibold">ชุดสำรอง</dt>
                <dd>
                  {KIND_LABEL[preview.backup.kind]} · {formatInstant(preview.backup.createdAt)}
                  <span className="block text-muted">{countsText(preview.backup.counts)}</span>
                </dd>
              </div>
              <div className="grid grid-cols-[150px_1fr] gap-3 px-3.5 py-2.5">
                <dt className="font-semibold">คลังตอนนี้</dt>
                <dd className="text-muted">{countsText(preview.current)}</dd>
              </div>
            </dl>
            <p>
              ข้อมูลในคลังตอนนี้จะถูกแทนด้วยข้อมูลจากชุดสำรอง สิ่งที่ทำหลังวันที่สำรองจะไม่อยู่ในคลังที่กู้คืน
              แต่คลังตอนนี้จะถูกเก็บเป็นชุด “คลังก่อนกู้คืน” ให้กู้กลับได้ ไฟล์รูปต้นฉบับไม่ถูกแตะ
            </p>
            {!preview.sameLibrary && (
              <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
                ชุดสำรองนี้มาจากคลังอื่น (ไม่ใช่คลังที่เปิดอยู่) ตรวจให้แน่ใจว่าเลือกถูกชุด
              </InlineNotice>
            )}
            {preview.needsUpgrade && <p className="text-muted">ชุดนี้สร้างด้วยแอปรุ่นก่อน ข้อมูลจะถูกปรับให้เข้ากับรุ่นนี้เมื่อเปิด</p>}
          </div>
        )}
      </Dialog>
    </>
  );
}

function BackupRow({ set, busy, onRestore }: { set: BackupInfo; busy: boolean; onRestore: () => void }) {
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3" data-testid="backup-row" data-kind={set.kind}>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold text-ink">
          {formatInstant(set.createdAt)} · {KIND_LABEL[set.kind]}
        </p>
        <p className="text-[13px] text-muted">
          {countsText(set.counts)} · {formatBytes(set.bytes)}
        </p>
      </div>
      <Button size="sm" busy={busy} onClick={onRestore} data-testid="backup-restore">
        กู้คืนชุดนี้
      </Button>
    </li>
  );
}
