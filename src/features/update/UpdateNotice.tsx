import { CircleArrowUp } from 'lucide-react';
import { Tooltip } from '@/components/ui/Overlay';
import { useUpdate } from './updates';

/** ปุ่มในเมนูซ้ายเมื่อรุ่นใหม่ดาวน์โหลดเสร็จ: อยู่จนกว่าจะอัปเดต ไม่บังการทำงาน */
export function UpdateNotice({ collapsed }: { collapsed: boolean }) {
  const status = useUpdate((s) => s.status);
  const setDialogOpen = useUpdate((s) => s.setDialogOpen);
  if (!status?.version || (status.state !== 'ready' && status.state !== 'installing')) return null;

  const label = `อัปเดตเป็นรุ่น ${status.version}`;
  if (collapsed) {
    return (
      <Tooltip label={`${label} — ดาวน์โหลดเสร็จแล้ว`} side="right">
        <button
          type="button"
          aria-label={label}
          onClick={() => setDialogOpen(true)}
          data-testid="update-notice"
          className="relative mx-2.5 mb-3 flex h-10 shrink-0 items-center justify-center rounded-control border border-brand-line bg-brand-soft text-action hover:border-action/60"
        >
          <CircleArrowUp className="size-5" aria-hidden />
        </button>
      </Tooltip>
    );
  }
  return (
    <button
      type="button"
      onClick={() => setDialogOpen(true)}
      data-testid="update-notice"
      className="mx-3 mb-3 flex shrink-0 items-center gap-2.5 rounded-control border border-brand-line bg-brand-soft px-3 py-2.5 text-left hover:border-action/60"
    >
      <CircleArrowUp className="size-5 shrink-0 text-action" aria-hidden />
      <span className="min-w-0">
        <span className="block text-sm font-bold text-action">มีรุ่นใหม่พร้อมติดตั้ง</span>
        <span className="block truncate text-[12.5px] text-muted">รุ่น {status.version}</span>
      </span>
    </button>
  );
}
