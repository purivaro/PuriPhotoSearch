import { forwardRef } from 'react';
import { Eye, Pin, SlidersHorizontal, Sparkles } from 'lucide-react';
import { cn } from '@/lib/cn';
import { formatCount } from '@/lib/format';
import type { PersonSummary } from '@/lib/ipc/types';
import { FaceAvatar } from './FaceAvatar';

interface PersonCardProps {
  person: PersonSummary;
  /** การ์ดนี้เป็นเจ้าของแผงดูเร็วที่เปิดอยู่ */
  quickViewActive?: boolean;
  onOpen: (person: PersonSummary) => void;
  onQuickView: (person: PersonSummary, trigger: HTMLElement) => void;
  onTogglePin: (person: PersonSummary) => void;
  pinBusy?: boolean;
}

/**
 * การ์ดบุคคลสี่เหลี่ยมมน ใช้ชุดเดียวทั้งหน้าหลักและหน้าบุคคล (Blueprint 4.2, S05, S08)
 * คลิกการ์ด = เปิดรายละเอียด · ปุ่มตาหรือ Space = ดูเร็ว · ปักหมุดเป็นคำสั่งแยกที่ไม่เปิดการ์ด
 */
export const PersonCard = forwardRef<HTMLDivElement, PersonCardProps>(function PersonCard(
  { person, quickViewActive, onOpen, onQuickView, onTogglePin, pinBusy },
  ref,
) {
  return (
    <div
      ref={ref}
      data-testid="person-card"
      data-person-id={person.id}
      className={cn(
        'group relative rounded-card border bg-surface shadow-card transition-shadow hover:shadow-raised',
        quickViewActive ? 'border-brand ring-2 ring-brand' : 'border-line',
        person.archived && 'opacity-70',
      )}
    >
      <button
        type="button"
        data-testid="person-card-open"
        aria-label={`${person.displayName} ${formatCount(person.confirmedPhotoCount + person.aiPhotoCount)} รูป${person.aiPhotoCount > 0 ? ` (AI จัดให้ ${formatCount(person.aiPhotoCount)} รูป)` : ''}${person.uneditedPhotoCount ? ` ยังไม่แต่ง ${formatCount(person.uneditedPhotoCount)} รูป` : ''} เปิดรายละเอียด (กด Space เพื่อดูเร็ว)`}
        onClick={() => onOpen(person)}
        onKeyDown={(event) => {
          if (event.key === ' ') {
            event.preventDefault();
            onQuickView(person, event.currentTarget);
          }
        }}
        className="block w-full rounded-card text-left"
      >
        <FaceAvatar asset={person.coverAsset} name={person.displayName} shape="square" decorative className="aspect-[1/1.08] w-full rounded-t-[13px] text-4xl" />
        <span className="block px-3 pt-2.5 pb-3">
          <span className="block truncate pr-8 text-[15.5px] font-bold text-ink">{person.displayName}</span>
          <span
            className="block truncate pr-7 text-[13.5px] text-muted"
            title={person.aiPhotoCount > 0 ? `AI จัดให้ ${formatCount(person.aiPhotoCount)} รูป (ยังไม่มีคนตรวจ) · ยืนยันแล้ว ${formatCount(person.confirmedPhotoCount)} รูป` : undefined}
          >
            {/* ทุกรูปมาจาก AI: ใส่รูปประกายนำหน้าจำนวนเดียว; มีทั้งสองแบบ: บอกส่วนที่ AI จัดให้ต่อท้าย */}
            {person.aiPhotoCount > 0 && person.confirmedPhotoCount === 0 && <Sparkles className="mr-1 inline size-3.5 align-[-2px] text-action" aria-hidden />}
            {formatCount(person.confirmedPhotoCount + person.aiPhotoCount)} รูป
            {person.aiPhotoCount > 0 && person.confirmedPhotoCount > 0 && (
              <span className="ml-1.5 inline-flex items-center gap-0.5 font-semibold text-action" data-testid="person-ai-count">
                <Sparkles className="size-3.5" aria-hidden />
                {formatCount(person.aiPhotoCount)}
              </span>
            )}
            {person.nickname ? ` · ${person.nickname}` : ''}
            {person.archived ? ' · เก็บไว้' : ''}
          </span>
          {/* หน้าหลัก: รูปของคนนี้ที่ยังไม่แต่ง (แต่งตามคน — DECISIONS ข้อ 32) */}
          {person.uneditedPhotoCount !== undefined && person.confirmedPhotoCount + person.aiPhotoCount > 0 && (
            <span className={cn('mt-0.5 flex items-center gap-1 pr-7 text-[12.5px]', person.uneditedPhotoCount > 0 ? 'text-muted' : 'text-ok')} data-testid="person-unedited">
              <SlidersHorizontal className="size-3.5 shrink-0" aria-hidden />
              <span className="truncate">{person.uneditedPhotoCount > 0 ? `ยังไม่แต่ง ${formatCount(person.uneditedPhotoCount)} รูป` : 'แต่งครบแล้ว'}</span>
            </span>
          )}
        </span>
      </button>

      <button
        type="button"
        data-testid="person-pin"
        aria-pressed={person.pinned}
        aria-label={person.pinned ? `เลิกปักหมุด ${person.displayName}` : `ปักหมุด ${person.displayName}`}
        title={person.pinned ? 'เลิกปักหมุด' : 'ปักหมุด'}
        disabled={pinBusy}
        onClick={(event) => {
          event.stopPropagation();
          onTogglePin(person);
        }}
        className={cn(
          'absolute top-2 left-2 flex size-8 items-center justify-center rounded-lg border shadow-card transition-opacity',
          person.pinned
            ? 'border-brand-line bg-surface text-brand'
            : 'border-line bg-surface/95 text-muted opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 hover:text-ink focus-visible:opacity-100',
        )}
      >
        <Pin className="size-4" fill={person.pinned ? 'currentColor' : 'none'} aria-hidden />
      </button>

      <button
        type="button"
        data-testid="person-quickview"
        aria-label={`ดูเร็ว ${person.displayName}`}
        title="ดูเร็ว"
        aria-pressed={quickViewActive || undefined}
        onClick={(event) => {
          event.stopPropagation();
          onQuickView(person, event.currentTarget);
        }}
        className={cn(
          'absolute right-2 bottom-2.5 flex size-8 items-center justify-center rounded-lg transition-colors',
          quickViewActive ? 'bg-brand-soft text-action' : 'text-muted hover:bg-sunken hover:text-ink',
        )}
      >
        <Eye className="size-[18px]" aria-hidden />
      </button>
    </div>
  );
});

/** กริดการ์ดบุคคลแบบปรับตามพื้นที่จริง ไม่บังคับจำนวนคอลัมน์จนชื่อถูกตัด */
export const PERSON_GRID = 'grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-3.5';
