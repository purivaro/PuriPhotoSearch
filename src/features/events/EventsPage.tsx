import { useDeferredValue, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router';
import { FolderPlus, Search, SearchX, Sparkles } from 'lucide-react';
import { AddFolderButton, Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { Button } from '@/components/ui/Button';
import { EmptyState, ErrorState, InlineNotice, LoadingBlock } from '@/components/ui/Feedback';
import { Select, TextInput } from '@/components/ui/Form';
import { formatCount } from '@/lib/format';
import type { EventSummary } from '@/lib/ipc/types';
import { useEvents } from '@/lib/queries';
import { EVENT_GRID, EventCard } from './EventCard';

type EventSort = 'event_date' | 'added' | 'title';

function sortEvents(events: EventSummary[], sort: EventSort): EventSummary[] {
  const list = [...events];
  // ตัวตัดสินสุดท้ายเป็น ID เสมอ งานไม่ทราบวันที่จึงมีตำแหน่งแน่นอน (ท้ายรายการ)
  const byId = (a: EventSummary, b: EventSummary) => a.id.localeCompare(b.id);
  if (sort === 'title') return list.sort((a, b) => a.title.localeCompare(b.title, 'th') || byId(a, b));
  if (sort === 'added') return list.sort((a, b) => b.createdAt.localeCompare(a.createdAt) || byId(a, b));
  return list.sort((a, b) => {
    if (a.eventDate && b.eventDate) return b.eventDate.localeCompare(a.eventDate) || byId(a, b);
    if (a.eventDate) return -1;
    if (b.eventDate) return 1;
    return b.createdAt.localeCompare(a.createdAt) || byId(a, b);
  });
}

/** S12 — กิจกรรมและโฟลเดอร์: เห็นงานตามโฟลเดอร์ที่คุ้นเคย */
export function EventsPage() {
  useDocumentTitle('กิจกรรม / โฟลเดอร์');
  const [params, setParams] = useSearchParams();
  const [text, setText] = useState('');
  const [sort, setSort] = useState<EventSort>('event_date');
  const term = useDeferredValue(text.trim());
  const events = useEvents(term);
  const all = useEvents('');
  const libraryEmpty = all.data?.length === 0;

  const newIds = useMemo(() => (params.get('new') ?? '').split(',').filter(Boolean), [params]);
  const onlyNew = newIds.length > 0 && params.get('all') !== '1';
  const sorted = useMemo(() => sortEvents(events.data ?? [], sort), [events.data, sort]);
  const visible = onlyNew ? sorted.filter((event) => newIds.includes(event.id)) : sorted;

  const clearNew = () => {
    const next = new URLSearchParams(params);
    next.delete('new');
    next.delete('all');
    setParams(next, { replace: true });
  };

  return (
    <Page>
      <PageHeader title="กิจกรรม / โฟลเดอร์" subtitle="หนึ่งโฟลเดอร์งานเป็นหนึ่งกิจกรรม รูปต้นฉบับอยู่ในโฟลเดอร์เดิม" />

      {newIds.length > 0 && (
        <InlineNotice
          tone="brand"
          icon={<Sparkles aria-hidden />}
          className="mb-4"
          action={
            <span className="flex gap-2">
              {onlyNew ? (
                <Button size="sm" onClick={clearNew} data-testid="events-show-all">
                  แสดงกิจกรรมทั้งหมด
                </Button>
              ) : (
                <Button size="sm" onClick={clearNew}>
                  ปิด
                </Button>
              )}
            </span>
          }
        >
          <span className="font-semibold">เพิ่มกิจกรรมแล้ว {formatCount(newIds.length)} รายการ</span> กำลังเตรียมข้อมูลรูปเบื้องหลัง ใช้รูปที่พร้อมได้ทันที
          {onlyNew ? ' — กำลังแสดงเฉพาะชุดที่เพิ่งเพิ่ม' : ''}
        </InlineNotice>
      )}

      {!libraryEmpty && (
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <div className="relative min-w-[240px] flex-1 sm:max-w-[420px]">
            <Search className="pointer-events-none absolute top-1/2 left-3 size-[18px] -translate-y-1/2 text-muted" aria-hidden />
            <TextInput
              data-page-search
              data-testid="events-search"
              type="search"
              aria-label="ค้นชื่อกิจกรรม"
              value={text}
              onChange={(event) => setText(event.target.value)}
              placeholder="ค้นชื่อกิจกรรม"
              className="pl-10"
            />
          </div>
          <label className="flex items-center gap-2 text-sm text-muted">
            เรียงตาม
            <Select value={sort} onChange={(event) => setSort(event.target.value as EventSort)} aria-label="เรียงกิจกรรมตาม">
              <option value="event_date">วันกิจกรรมล่าสุด</option>
              <option value="added">วันเพิ่มล่าสุด</option>
              <option value="title">ชื่อกิจกรรม</option>
            </Select>
          </label>
          {events.data && <p className="ml-auto text-sm text-muted">{formatCount(visible.length)} กิจกรรม</p>}
        </div>
      )}

      {events.isPending ? (
        <LoadingBlock label="กำลังโหลดกิจกรรม…" />
      ) : events.isError ? (
        <ErrorState error={events.error} title="โหลดรายการกิจกรรมไม่สำเร็จ" onRetry={() => void events.refetch()} />
      ) : libraryEmpty ? (
        <EmptyState
          icon={<FolderPlus aria-hidden />}
          title="ยังไม่มีกิจกรรม"
          description="เพิ่มโฟลเดอร์ที่เก็บรูปของงาน เช่น “691118 งานอบรม AI” — หนึ่งโฟลเดอร์งานจะเป็นหนึ่งกิจกรรม โดยรูปยังอยู่ที่เดิม"
          action={<AddFolderButton size="lg" />}
        />
      ) : visible.length === 0 ? (
        <EmptyState
          icon={<SearchX aria-hidden />}
          title={term ? `ไม่พบกิจกรรมที่ชื่อตรงกับ “${term}”` : 'ไม่มีกิจกรรมในมุมมองนี้'}
          description={term ? 'คลังยังมีกิจกรรมอื่นอยู่ ลองล้างคำค้นเพื่อดูทั้งหมด' : undefined}
          action={
            <>
              {term && (
                <Button onClick={() => setText('')} data-testid="events-clear-search">
                  ล้างคำค้น
                </Button>
              )}
              {onlyNew && <Button onClick={clearNew}>แสดงกิจกรรมทั้งหมด</Button>}
            </>
          }
        />
      ) : (
        <ul className={EVENT_GRID} data-testid="event-grid">
          {visible.map((event) => (
            <li key={event.id}>
              <EventCard event={event} highlighted={newIds.includes(event.id) && !onlyNew} />
            </li>
          ))}
        </ul>
      )}
    </Page>
  );
}
