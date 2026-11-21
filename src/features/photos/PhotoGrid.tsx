import { useEffect, useRef } from 'react';
import { Check, CloudOff, SlidersHorizontal, Sparkles, Users } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Feedback';
import { cn } from '@/lib/cn';
import { formatCount } from '@/lib/format';
import type { PhotoFlag, PhotoResult } from '@/lib/ipc/types';
import { THUMB_SIZES, useSettings } from '@/stores/settings';
import { FlagBadge, flagForKey } from './PhotoFlags';
import { PhotoThumb } from './PhotoThumb';
import { EMPTY_SELECTION, isSelected, selectLoaded, selectRange, selectionCount, toggle, type Selection } from './selection';

/** ป้ายคนในรูป: คนที่ยืนยันแล้ว และคนที่ AI จัดให้ (มีรูปประกายบอกว่ายังไม่มีคนตรวจ) */
function PeopleBadge({ photo }: { photo: PhotoResult }) {
  const confirmed = photo.confirmedPeople.map((p) => p.displayName);
  const byAi = [...new Set(photo.suggestedPeople.map((s) => s.person.displayName))].filter((name) => !confirmed.includes(name));
  const names = [...confirmed, ...byAi];
  if (names.length === 0) return null;
  const title = [confirmed.length ? `ยืนยันแล้ว: ${confirmed.join(', ')}` : null, byAi.length ? `AI จัดให้ (ยังไม่มีคนตรวจ): ${byAi.join(', ')}` : null]
    .filter(Boolean)
    .join(' · ');
  return (
    <span
      title={title}
      data-ai={byAi.length > 0 || undefined}
      className="absolute bottom-1.5 left-1.5 inline-flex max-w-[70%] items-center gap-1 rounded-md bg-black/65 px-1.5 py-0.5 text-[11.5px] font-medium text-white"
    >
      {byAi.length > 0 ? <Sparkles className="size-3 shrink-0 text-brand" aria-label="AI จัดให้" /> : <Users className="size-3 shrink-0" aria-hidden />}
      <span className="truncate">{names.length === 1 ? names[0] : `${names.length} คน`}</span>
    </span>
  );
}

interface PhotoGridProps {
  items: PhotoResult[];
  /** ชื่อ grid สำหรับ screen reader เช่น "รูปในกิจกรรม งานอบรม AI" */
  label: string;
  selection: Selection;
  onSelectionChange: (selection: Selection) => void;
  onOpen: (photo: PhotoResult) => void;
  hasMore: boolean;
  loadingMore: boolean;
  onLoadMore: () => void;
  /** แสดงชื่อกิจกรรมใต้ชื่อไฟล์ (ใช้เมื่อผลมาจากหลายกิจกรรม) */
  showEvent?: boolean;
  total: number;
  /** คัดรูปที่เลือกด้วยคีย์ P / X / U (DECISIONS ข้อ 33) */
  onFlag?: (flag: PhotoFlag | null) => void;
}

/** กริดรูปแบบคอลัมน์สม่ำเสมอ เลือกหลายรูปด้วย checkbox / Shift / ⌘A ได้ */
export function PhotoGrid({ items, label, selection, onSelectionChange, onOpen, hasMore, loadingMore, onLoadMore, showEvent, total, onFlag }: PhotoGridProps) {
  const sentinel = useRef<HTMLDivElement>(null);
  const loadMoreRef = useRef(onLoadMore);
  loadMoreRef.current = onLoadMore;
  const anySelected = selectionCount(selection) > 0;
  const thumbSize = useSettings((s) => s.thumbSize);
  // รูปเล็กมาก: ซ่อนชื่อไฟล์ (ยังอยู่ใน title/aria-label) ให้เห็นรูปได้มากขึ้น
  const compact = thumbSize === 0;

  useEffect(() => {
    const el = sentinel.current;
    if (!el || !hasMore) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) loadMoreRef.current();
      },
      { rootMargin: '600px 0px' },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [hasMore, items.length]);

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'a') {
      // ⌘A เลือกเฉพาะรูปที่โหลดอยู่ใน grid; "เลือกผลทั้งหมด" เป็นปุ่มแยกในแถบเลือก
      event.preventDefault();
      onSelectionChange(selectLoaded(selection, items.map((p) => p.id)));
    } else if (event.key === 'Escape' && anySelected) {
      event.preventDefault();
      event.stopPropagation();
      onSelectionChange(EMPTY_SELECTION);
    } else if (anySelected && onFlag) {
      const flag = flagForKey(event);
      if (flag !== undefined) {
        event.preventDefault();
        onFlag(flag);
      }
    }
  };

  const onToggle = (photo: PhotoResult, shift: boolean) => {
    onSelectionChange(shift ? selectRange(selection, items.map((p) => p.id), photo.id) : toggle(selection, photo.id));
  };

  return (
    <div onKeyDown={onKeyDown}>
      <ul
        aria-label={label}
        data-testid="photo-grid"
        data-size={thumbSize}
        style={{ gridTemplateColumns: `repeat(auto-fill, minmax(${THUMB_SIZES[thumbSize]}px, 1fr))` }}
        className={cn('grid', compact ? 'gap-2' : 'gap-x-3 gap-y-3.5')}
      >
        {items.map((photo) => {
          const selected = isSelected(selection, photo.id);
          return (
            <li key={photo.id} className="group relative min-w-0" data-testid="photo-card" data-photo-id={photo.id}>
              <button
                type="button"
                data-testid="photo-card-open"
                aria-label={`เปิดรูป ${photo.filename}${showEvent ? ` จาก ${photo.eventTitle}` : ''}`}
                onClick={(event) => {
                  // Shift/⌘+คลิกบนรูป = เลือก ไม่เปิดตัวดูภาพ
                  if (event.shiftKey || event.metaKey || event.ctrlKey) onToggle(photo, event.shiftKey);
                  else onOpen(photo);
                }}
                className={cn('relative block w-full overflow-hidden rounded-xl bg-sunken', selected && 'ring-[3px] ring-brand')}
              >
                <PhotoThumb
                  photo={photo}
                  className={cn('aspect-[3/2] w-full transition-[transform,opacity] duration-200 group-hover:scale-[1.015]', photo.flag === 'reject' && 'opacity-40 group-hover:opacity-80')}
                />
                {!photo.available && (
                  <span className="absolute right-1.5 bottom-1.5 inline-flex items-center gap-1 rounded-md bg-black/65 px-1.5 py-0.5 text-[11.5px] font-medium text-white">
                    <CloudOff className="size-3" aria-hidden /> ต้นฉบับไม่พร้อม
                  </span>
                )}
                <PeopleBadge photo={photo} />
                {(photo.flag || photo.editId || photo.raw || photo.rawPair) && (
                  <span className="absolute top-1.5 right-1.5 flex items-center gap-1">
                    {photo.flag && <FlagBadge flag={photo.flag} compact={compact} ai={photo.flagAi} />}
                    {photo.editId && (
                      <span
                        title="แต่งในแอปแล้ว — ไฟล์ต้นฉบับไม่ถูกแตะ"
                        data-testid="photo-card-edited"
                        className="inline-flex items-center gap-1 rounded-md bg-black/65 px-1.5 py-0.5 text-[11px] font-semibold text-white"
                      >
                        <SlidersHorizontal className="size-3 shrink-0" aria-hidden />
                        {compact ? <span className="sr-only">แต่งแล้ว</span> : 'แต่งแล้ว'}
                      </span>
                    )}
                    {(photo.raw || photo.rawPair) && (
                      <span
                        title={
                          photo.rawPair
                            ? `มีไฟล์ RAW คู่ (${photo.rawPair}) — ภาพที่เห็นคือ JPEG แต่งจาก RAW ได้`
                            : 'ไฟล์ RAW จากกล้อง — แต่งจากข้อมูลเซนเซอร์ ส่งออกเป็น JPEG'
                        }
                        data-testid="photo-card-raw"
                        className="rounded-md bg-black/65 px-1.5 py-0.5 text-[11px] font-bold tracking-wide text-white"
                      >
                        {photo.rawPair ? 'RAW+JPEG' : 'RAW'}
                      </span>
                    )}
                  </span>
                )}
              </button>
              <button
                type="button"
                role="checkbox"
                aria-checked={selected}
                data-testid="photo-card-select"
                aria-label={`เลือกรูป ${photo.filename}`}
                onClick={(event) => onToggle(photo, event.shiftKey)}
                className={cn(
                  'absolute top-2 left-2 flex size-6 items-center justify-center rounded-md border-2 shadow-card transition-opacity',
                  selected ? 'border-action bg-action text-on-action' : 'border-white bg-black/25 text-transparent hover:bg-black/40',
                  !selected && !anySelected && 'opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 focus-visible:opacity-100',
                )}
              >
                <Check className="size-4" strokeWidth={3} aria-hidden />
              </button>
              {!compact && (
                <p className="mt-1 truncate px-0.5 text-[12.5px] text-muted" title={photo.filename}>
                  {photo.filename}
                </p>
              )}
              {showEvent && !compact && (
                <p className="-mt-0.5 truncate px-0.5 text-[12px] text-faint" title={photo.eventTitle}>
                  {photo.eventTitle}
                </p>
              )}
            </li>
          );
        })}
      </ul>
      <div ref={sentinel} className="flex min-h-10 items-center justify-center pt-5">
        {hasMore ? (
          loadingMore ? (
            <span role="status" className="flex items-center gap-2 text-sm text-muted">
              <Spinner className="size-4" /> กำลังโหลดรูปเพิ่ม…
            </span>
          ) : (
            <Button size="sm" onClick={onLoadMore} data-testid="photo-grid-load-more">
              โหลดเพิ่ม (แสดงแล้ว {formatCount(items.length)} จาก {formatCount(total)} รูป)
            </Button>
          )
        ) : (
          items.length > 24 && <span className="text-[13px] text-faint">แสดงครบ {formatCount(items.length)} รูป</span>
        )}
      </div>
    </div>
  );
}
