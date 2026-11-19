import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { CircleArrowUp, CircleCheck, Download, RefreshCw, TriangleAlert, Wrench } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { InlineNotice, ProgressBar, StatusBadge } from '@/components/ui/Feedback';
import { Switch } from '@/components/ui/Form';
import { errorMessage, formatBytes, formatInstant } from '@/lib/format';
import type { UpdateStatus } from '@/lib/ipc/types';
import { useSettings } from '@/stores/settings';
import { checkForUpdate, useUpdate } from './updates';

const CARD = 'rounded-card border border-line bg-surface p-6 shadow-card';

const BADGE: Partial<Record<UpdateStatus['state'], { tone: 'ok' | 'brand' | 'warn' | 'neutral'; label: string; icon: typeof CircleCheck }>> = {
  up_to_date: { tone: 'ok', label: 'เป็นรุ่นล่าสุด', icon: CircleCheck },
  checking: { tone: 'neutral', label: 'กำลังตรวจ', icon: RefreshCw },
  downloading: { tone: 'brand', label: 'กำลังดาวน์โหลดรุ่นใหม่', icon: Download },
  ready: { tone: 'brand', label: 'มีรุ่นใหม่พร้อมติดตั้ง', icon: CircleArrowUp },
  installing: { tone: 'brand', label: 'กำลังติดตั้ง', icon: CircleArrowUp },
  failed: { tone: 'warn', label: 'ตรวจไม่สำเร็จ', icon: TriangleAlert },
};

const FOUND: UpdateStatus['state'][] = ['downloading', 'ready', 'installing'];

/** หมวด "อัปเดตแอป" ในตั้งค่า: รุ่นที่ใช้ สถานะตามจริงจากเปลือกแอป ตรวจเองได้ และเปิด/ปิดการตรวจอัตโนมัติ */
export function UpdateSection({ appVersion }: { appVersion: string }) {
  const status = useUpdate((s) => s.status);
  const setDialogOpen = useUpdate((s) => s.setDialogOpen);
  const autoUpdate = useSettings((s) => s.autoUpdate);
  const setAutoUpdate = useSettings((s) => s.setAutoUpdate);
  const check = useMutation({ mutationFn: checkForUpdate, onError: (error) => toast.error(errorMessage(error)) });

  // เปิดในเบราว์เซอร์ระหว่างพัฒนา (ไม่มีเปลือกแอป) หรือ debug build: ไม่มีตัวอัปเดต
  const available = status !== null && status.state !== 'unsupported';
  const badge = status ? BADGE[status.state] : undefined;
  const checkedAt = status?.checkedAt ? formatInstant(new Date(status.checkedAt).toISOString()) : null;
  const checking = check.isPending || status?.state === 'checking';

  return (
    <section className={CARD} data-testid="settings-update">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-lg font-bold text-ink">อัปเดตแอป</h2>
        {badge && (
          <StatusBadge tone={badge.tone} icon={<badge.icon aria-hidden />}>
            {badge.label}
          </StatusBadge>
        )}
      </div>
      <p className="mt-0.5 text-sm text-muted" data-testid="update-current">
        รุ่นที่ใช้อยู่ {status?.currentVersion ?? appVersion}
        {checkedAt && ` · ตรวจล่าสุด ${checkedAt}`}
      </p>

      <div className="mt-3 space-y-3" aria-live="polite">
        {!available && (
          <InlineNotice icon={<Wrench aria-hidden />}>
            แอปที่เปิดอยู่เป็นรุ่นสำหรับพัฒนา จึงไม่ตรวจหารุ่นใหม่ — การอัปเดตทำงานในแอปที่ติดตั้งจากตัวติดตั้งเท่านั้น
          </InlineNotice>
        )}
        {status?.state === 'downloading' && (
          <div data-testid="update-downloading">
            <ProgressBar value={status.downloaded} max={status.total ?? 0} indeterminate={!status.total} label={`ดาวน์โหลดรุ่น ${status.version}`} />
            <p className="mt-1.5 text-[13px] text-muted">
              กำลังดาวน์โหลดรุ่น {status.version} เบื้องหลัง — {formatBytes(status.downloaded)}
              {status.total ? ` จาก ${formatBytes(status.total)}` : ''} ใช้แอปต่อได้ตามปกติ
            </p>
          </div>
        )}
        {(status?.state === 'ready' || status?.state === 'installing') && (
          <InlineNotice
            tone="brand"
            icon={<CircleArrowUp aria-hidden />}
            action={
              <Button variant="primary" size="sm" onClick={() => setDialogOpen(true)} data-testid="update-open">
                อัปเดตเป็นรุ่น {status.version}
              </Button>
            }
          >
            รุ่น {status.version} ดาวน์โหลดและตรวจลายเซ็นแล้ว อัปเดตได้เมื่อสะดวก
          </InlineNotice>
        )}
        {status?.state === 'failed' && status.message && (
          <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
            {status.message}
          </InlineNotice>
        )}
      </div>

      {available && (
        <>
          {/* พบรุ่นใหม่แล้วไม่มีอะไรให้ตรวจซ้ำ: เหลือเฉพาะความคืบหน้าหรือปุ่มอัปเดตด้านบน */}
          {!FOUND.includes(status.state) && (
            <Button className="mt-4" icon={<RefreshCw className="size-4" aria-hidden />} busy={checking} onClick={() => check.mutate()} data-testid="update-check">
              {checking ? 'กำลังตรวจ…' : status.state === 'failed' ? 'ลองตรวจใหม่' : 'ตรวจหารุ่นใหม่'}
            </Button>
          )}
          <div className="mt-4 border-t border-line pt-4">
            <Switch
              checked={autoUpdate}
              onCheckedChange={setAutoUpdate}
              testId="update-auto"
              label="ตรวจและดาวน์โหลดรุ่นใหม่ให้เอง"
              description="ตอนเปิดแอปและทุก 6 ชั่วโมง แอปจะถาม GitHub ว่ามีรุ่นใหม่ไหม แล้วดาวน์โหลดไว้ให้ การติดตั้งเกิดเมื่อกดปุ่มเท่านั้น ไม่มีการส่งรูป ชื่อบุคคล หรือข้อมูลในคลังออกไป"
            />
          </div>
        </>
      )}
    </section>
  );
}
