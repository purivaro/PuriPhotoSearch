import { Link } from 'react-router';
import { Calendar, CircleCheck, CloudOff, Image as ImageIcon, ScanFace, TriangleAlert } from 'lucide-react';
import { Spinner, StatusBadge } from '@/components/ui/Feedback';
import { cn } from '@/lib/cn';
import { formatCount, formatThaiDate } from '@/lib/format';
import { assetUrl } from '@/lib/ipc/client';
import type { EventSummary } from '@/lib/ipc/types';
import { useCapabilities } from '@/lib/queries';

/** สถานะ "ข้อมูลรูปพร้อม" ของกิจกรรม — แยกจากความพร้อมของโฟลเดอร์ต้นทาง */
export function EventStatus({ event }: { event: EventSummary }) {
  const faceReady = useCapabilities().data?.faceAnalysis.available ?? false;
  if (event.activeJobId) {
    // ภาพย่อครบแล้วแต่งานยังเดิน = กำลังวิเคราะห์ใบหน้า (เป็นคนละสถานะกับ "ข้อมูลรูปพร้อม")
    if (faceReady && event.photoCount > 0 && event.thumbReadyCount >= event.photoCount && event.faceAnalyzedCount < event.photoCount) {
      return (
        <StatusBadge tone="brand" icon={<Spinner className="size-3.5" />}>
          กำลังวิเคราะห์ใบหน้า {formatCount(event.faceAnalyzedCount)}/{formatCount(event.photoCount)}
        </StatusBadge>
      );
    }
    return (
      <StatusBadge tone="brand" icon={<Spinner className="size-3.5" />}>
        กำลังเตรียมรูป {formatCount(event.thumbReadyCount)}/{formatCount(event.photoCount)}
      </StatusBadge>
    );
  }
  if (event.photoCount === 0) return <StatusBadge tone="neutral">ยังไม่มีรูป</StatusBadge>;
  if (event.failedCount > 0) {
    return (
      <StatusBadge tone="warn" icon={<TriangleAlert aria-hidden />}>
        อ่านไม่ได้ {formatCount(event.failedCount)} รูป
      </StatusBadge>
    );
  }
  if (event.thumbReadyCount < event.photoCount) {
    return (
      <StatusBadge tone="neutral">
        ข้อมูลรูปพร้อม {formatCount(event.thumbReadyCount)}/{formatCount(event.photoCount)}
      </StatusBadge>
    );
  }
  return (
    <StatusBadge tone="ok" icon={<CircleCheck aria-hidden />}>
      ข้อมูลรูปพร้อม
    </StatusBadge>
  );
}

export function EventCard({ event, highlighted }: { event: EventSummary; highlighted?: boolean }) {
  const date = formatThaiDate(event.eventDate);
  // แสดงเฉพาะเมื่อวิเคราะห์ได้จริงและยังไม่ครบ: "ข้อมูลรูปพร้อม" ไม่ได้แปลว่าวิเคราะห์ใบหน้าแล้ว
  const faceReady = useCapabilities().data?.faceAnalysis.available ?? false;
  const facePending = faceReady && event.photoCount > 0 && event.faceAnalyzedCount < event.photoCount;
  return (
    <Link
      to={`/events/${event.id}`}
      data-testid="event-card"
      data-event-id={event.id}
      title={event.folderPath}
      className={cn(
        'group block overflow-hidden rounded-card border bg-surface shadow-card transition-shadow hover:shadow-raised',
        highlighted ? 'border-brand ring-2 ring-brand/40' : 'border-line',
      )}
    >
      <div className="relative aspect-[16/9.5] overflow-hidden bg-sunken">
        {event.coverPhotoId ? (
          <img src={assetUrl('thumb', event.coverPhotoId)} alt="" loading="lazy" className="size-full object-cover transition-transform duration-300 group-hover:scale-[1.02]" />
        ) : (
          <div className="flex size-full flex-col items-center justify-center gap-1 text-faint">
            <ImageIcon className="size-8" aria-hidden />
            <span className="text-[12.5px] text-muted">{event.photoCount === 0 ? 'ยังไม่มีรูป' : 'กำลังเตรียมภาพปก'}</span>
          </div>
        )}
        {!event.sourceAvailable && (
          <span className="absolute top-2 left-2 inline-flex items-center gap-1 rounded-md bg-black/70 px-2 py-1 text-[12px] font-semibold text-white">
            <CloudOff className="size-3.5" aria-hidden /> โฟลเดอร์ไม่พร้อม
          </span>
        )}
      </div>
      <div className="px-4 pt-3 pb-3.5">
        <div className="flex items-start justify-between gap-2">
          <h3 className="min-w-0 truncate text-[17px] font-bold text-ink">{event.title}</h3>
          <span className="shrink-0">
            <EventStatus event={event} />
          </span>
        </div>
        <p className="mt-1 flex items-center gap-x-3 text-[13.5px] text-muted">
          <span className="inline-flex items-center gap-1.5">
            <Calendar className="size-4" aria-hidden />
            {date ?? 'ยังไม่ระบุวันที่'}
          </span>
          <span className="h-3.5 w-px bg-line-strong" aria-hidden />
          <span className="inline-flex items-center gap-1.5">
            <ImageIcon className="size-4" aria-hidden />
            {formatCount(event.photoCount)} รูป
          </span>
        </p>
        {facePending && (
          <p className="mt-1 flex items-center gap-1.5 text-[13px] text-muted" data-testid="event-face-progress">
            <ScanFace className="size-4" aria-hidden />
            วิเคราะห์ใบหน้า {formatCount(event.faceAnalyzedCount)}/{formatCount(event.photoCount)}
          </p>
        )}
      </div>
    </Link>
  );
}

export const EVENT_GRID = 'grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-4';
