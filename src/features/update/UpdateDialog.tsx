import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Clock } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { InlineNotice } from '@/components/ui/Feedback';
import { errorMessage, formatCount } from '@/lib/format';
import { updateCommand } from '@/lib/ipc/client';
import { isJobActive, useActiveJobs } from '@/lib/queries';
import { useUpdate } from './updates';

/** ยืนยันก่อนติดตั้งรุ่นใหม่: บอกว่าแอปจะเปิดใหม่ มีอะไรใหม่ และงานเบื้องหลังที่กำลังทำจะเป็นอย่างไร */
export function UpdateDialog() {
  const status = useUpdate((s) => s.status);
  const open = useUpdate((s) => s.dialogOpen);
  const setOpen = useUpdate((s) => s.setDialogOpen);
  const activeJobs = useActiveJobs().data?.filter(isJobActive).length ?? 0;

  const install = useMutation({
    // สำเร็จแล้วแอปปิดและเปิดใหม่เอง คำสั่งนี้จึงคืนค่าเฉพาะเมื่อติดตั้งไม่สำเร็จ
    mutationFn: () => updateCommand('install'),
    onError: (error) => toast.error(errorMessage(error), { duration: 15_000 }),
  });

  if (!status?.version || (status.state !== 'ready' && status.state !== 'installing')) return null;
  const busy = install.isPending || status.state === 'installing';

  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      title={`อัปเดตเป็นรุ่น ${status.version}`}
      description={`ตอนนี้ใช้รุ่น ${status.currentVersion} — แอปจะปิด ติดตั้งรุ่นใหม่ แล้วเปิดขึ้นมาเองอีกครั้ง`}
      dismissible={!busy}
      footer={
        <>
          <Button variant="secondary" onClick={() => setOpen(false)} disabled={busy}>
            ไว้ทีหลัง
          </Button>
          <Button variant="primary" onClick={() => install.mutate()} busy={busy} data-testid="update-install">
            {busy ? 'กำลังติดตั้ง…' : 'อัปเดตและเปิดใหม่'}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        {status.notes && (
          <section aria-label="มีอะไรใหม่" className="rounded-xl border border-line bg-sunken px-4 py-3">
            <h3 className="text-sm font-bold text-ink">มีอะไรใหม่</h3>
            <p className="mt-1 max-h-[220px] overflow-y-auto text-sm whitespace-pre-line text-ink" data-testid="update-notes">
              {status.notes}
            </p>
          </section>
        )}
        {activeJobs > 0 && (
          <InlineNotice tone="warn" icon={<Clock aria-hidden />}>
            มีงานเบื้องหลังกำลังทำอยู่ {formatCount(activeJobs)} งาน — งานจะหยุดชั่วคราว แล้วทำต่อเองหลังแอปเปิดใหม่
          </InlineNotice>
        )}
        <p className="text-[13px] text-muted">คลังข้อมูล ชื่อบุคคล และไฟล์รูปต้นฉบับไม่ถูกแตะระหว่างอัปเดต</p>
      </div>
    </Dialog>
  );
}
