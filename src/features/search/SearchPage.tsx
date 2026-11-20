import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router';
import { Calendar, Folder, Search, SearchX, UserPlus, X } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { Button } from '@/components/ui/Button';
import { EmptyState, ErrorState, Spinner } from '@/components/ui/Feedback';
import { Select, Tabs, TextInput } from '@/components/ui/Form';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/Overlay';
import { FaceAvatar } from '@/features/people/FaceAvatar';
import { PersonPicker } from '@/features/people/PersonPicker';
import { PhotoResults } from '@/features/photos/PhotoResults';
import { cn } from '@/lib/cn';
import { formatCount, formatThaiDate } from '@/lib/format';
import type { PersonMode, SearchParams } from '@/lib/ipc/types';
import { PHOTO_PAGE_SIZE, useEvents, usePeople } from '@/lib/queries';
import { DEFAULT_FILTERS, filterIdentity, filtersToQuery, filtersToSearch, hasActiveFilters, hasDateFilter, parseFilters, type SearchFilters } from './searchParams';

const CHIP = 'inline-flex h-9 items-center gap-1.5 rounded-full border border-brand-line bg-brand-soft pr-1 text-sm font-semibold text-action';

/** S06 — ค้นหารูปและผลลัพธ์: ค้นข้ามกิจกรรมด้วยคน กิจกรรม วันที่ และชื่อไฟล์ (เงื่อนไขอยู่ใน URL) */
export function SearchPage() {
  useDocumentTitle('ค้นหารูป');
  const [params, setParams] = useSearchParams();
  const filters = useMemo(() => parseFilters(params), [params]);
  const [text, setText] = useState(filters.text);
  const people = usePeople({ includeArchived: true });
  const events = useEvents('');
  const [personOpen, setPersonOpen] = useState(false);

  const update = (patch: Partial<SearchFilters>) => setParams(filtersToSearch({ ...filters, ...patch }), { replace: true });

  // URL เปลี่ยนจากภายนอก (เช่นเลือกจาก ⌘K) → ช่องพิมพ์ตามให้ตรง
  useEffect(() => setText(filters.text), [filters.text]);
  // พิมพ์แล้วรอจังหวะสั้น ๆ ก่อนค้น; ผลของคำเก่าไม่ทับคำใหม่เพราะ key ของ query ผูกกับเงื่อนไข
  useEffect(() => {
    if (text.trim() === filters.text.trim()) return;
    const timer = window.setTimeout(() => update({ text }), 280);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  const query = useMemo(() => filtersToQuery(filters, PHOTO_PAGE_SIZE), [filters]);
  const personById = useMemo(() => new Map((people.data ?? []).map((p) => [p.id, p])), [people.data]);
  const eventById = useMemo(() => new Map((events.data ?? []).map((e) => [e.id, e])), [events.data]);
  const active = hasActiveFilters(filters);
  const dateActive = hasDateFilter(filters);
  const clearAll = () => {
    setText('');
    setParams(filtersToSearch({ ...DEFAULT_FILTERS, sort: filters.sort }), { replace: true });
  };

  const dateLabel = dateActive
    ? `${filters.dateField === 'event_date' ? 'วันกิจกรรม' : 'วันถ่าย'} ${formatThaiDate(filters.from) ?? '…'} – ${formatThaiDate(filters.to) ?? '…'}${filters.includeUnknownDate ? ' + ไม่ทราบวันที่' : ''}`
    : 'ช่วงวันที่';

  return (
    <Page>
      <PageHeader title="ค้นหารูป" subtitle="ค้นจากชื่อคน กิจกรรม วันที่ และชื่อไฟล์ที่อยู่ในคลัง" />

      <div className="mb-3 rounded-card border border-line bg-surface p-4 shadow-card">
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-3.5 size-5 -translate-y-1/2 text-muted" aria-hidden />
          <TextInput
            data-page-search
            data-testid="search-input"
            type="search"
            aria-label="คำค้น"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') update({ text });
            }}
            placeholder="พิมพ์ชื่อคน ชื่อกิจกรรม หรือชื่อไฟล์"
            className="h-12 pl-11 text-[15px]"
          />
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-2" role="group" aria-label="ตัวกรอง">
          {filters.personIds.map((id) => {
            const person = personById.get(id);
            return (
              <span key={id} className={cn(CHIP, 'pl-1')} data-testid="search-person-chip" data-person-id={id}>
                <FaceAvatar asset={person?.coverAsset ?? null} name={person?.displayName ?? ''} decorative className="size-7 text-xs" />
                <span className="max-w-[160px] truncate">{person?.displayName ?? (people.isPending ? 'กำลังโหลด…' : 'บุคคลที่ไม่พบในคลัง')}</span>
                <button
                  type="button"
                  aria-label={`เอา ${person?.displayName ?? 'บุคคลนี้'} ออกจากตัวกรอง`}
                  onClick={() => update({ personIds: filters.personIds.filter((p) => p !== id) })}
                  className="flex size-7 items-center justify-center rounded-full hover:bg-brand-line/50"
                >
                  <X className="size-3.5" aria-hidden />
                </button>
              </span>
            );
          })}
          <Popover open={personOpen} onOpenChange={setPersonOpen}>
            <PopoverTrigger asChild>
              <Button size="sm" icon={<UserPlus className="size-4" aria-hidden />} className="h-9 rounded-full" data-testid="search-add-person">
                {filters.personIds.length ? 'เพิ่มบุคคล' : 'บุคคล'}
              </Button>
            </PopoverTrigger>
            <PopoverContent aria-label="เพิ่มบุคคลในตัวกรอง">
              <PersonPicker
                allowCreate={false}
                excludeIds={filters.personIds}
                placeholder="พิมพ์ชื่อเพื่อค้นหาบุคคล"
                onPick={(choice) => {
                  if (choice.kind !== 'existing') return;
                  update({ personIds: [...filters.personIds, choice.person.id] });
                  setPersonOpen(false);
                }}
              />
            </PopoverContent>
          </Popover>

          {filters.personIds.length > 1 && (
            <Tabs<PersonMode>
              label="เงื่อนไขเมื่อเลือกหลายคน"
              variant="pill"
              value={filters.personMode}
              onChange={(personMode) => update({ personMode })}
              items={[
                { value: 'all', label: 'มีทุกคนในภาพเดียวกัน' },
                { value: 'any', label: 'มีคนใดคนหนึ่ง' },
              ]}
              testId="search-person-mode"
            />
          )}

          <span className="mx-1 h-6 w-px bg-line" aria-hidden />

          {filters.eventIds.map((id) => (
            <span key={id} className={cn(CHIP, 'pl-3')} data-testid="search-event-chip">
              <Folder className="size-3.5" aria-hidden />
              <span className="max-w-[180px] truncate">{eventById.get(id)?.title ?? (events.isPending ? 'กำลังโหลด…' : 'กิจกรรมที่ไม่พบในคลัง')}</span>
              <button type="button" aria-label="เอากิจกรรมนี้ออกจากตัวกรอง" onClick={() => update({ eventIds: filters.eventIds.filter((e) => e !== id) })} className="flex size-7 items-center justify-center rounded-full hover:bg-brand-line/50">
                <X className="size-3.5" aria-hidden />
              </button>
            </span>
          ))}
          <EventFilter selected={filters.eventIds} onChange={(eventIds) => update({ eventIds })} />

          <Popover>
            <PopoverTrigger asChild>
              <Button size="sm" icon={<Calendar className="size-4" aria-hidden />} className={cn('h-9 rounded-full', dateActive && 'border-brand-line bg-brand-soft text-action')} data-testid="search-date">
                {dateLabel}
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-[300px] space-y-3 p-4" aria-label="กรองตามช่วงวันที่">
              <Tabs<SearchFilters['dateField']>
                label="ชนิดวันที่"
                variant="pill"
                value={filters.dateField}
                onChange={(dateField) => update({ dateField })}
                items={[
                  { value: 'photo_taken', label: 'วันถ่าย' },
                  { value: 'event_date', label: 'วันกิจกรรม' },
                ]}
              />
              <label className="block text-sm font-semibold text-ink">
                ตั้งแต่
                <TextInput type="date" className="mt-1" value={filters.from} max={filters.to || undefined} onChange={(e) => update({ from: e.target.value })} />
              </label>
              <label className="block text-sm font-semibold text-ink">
                ถึง
                <TextInput type="date" className="mt-1" value={filters.to} min={filters.from || undefined} onChange={(e) => update({ to: e.target.value })} />
              </label>
              <label className={cn('flex items-center gap-2 text-sm', dateActive ? 'cursor-pointer text-ink' : 'text-faint')}>
                <input type="checkbox" className="size-4 accent-[var(--action)]" disabled={!dateActive} checked={filters.includeUnknownDate} onChange={(e) => update({ includeUnknownDate: e.target.checked })} />
                รวมรูปที่ไม่ทราบ{filters.dateField === 'event_date' ? 'วันกิจกรรม' : 'วันถ่าย'}
              </label>
              <p className="text-[12.5px] text-muted">{filters.dateField === 'photo_taken' ? 'วันถ่ายมาจากข้อมูลในไฟล์ (EXIF) ไม่ใช้วันแก้ไขไฟล์แทน' : 'วันกิจกรรมคือวันที่ตั้งไว้ให้กิจกรรมในแอป'}</p>
              {dateActive && (
                <Button size="sm" variant="ghost" onClick={() => update({ from: '', to: '', includeUnknownDate: false })}>
                  ล้างช่วงวันที่
                </Button>
              )}
            </PopoverContent>
          </Popover>

          <Select value={filters.faceFilter} onChange={(e) => update({ faceFilter: e.target.value as SearchParams['faceFilter'] })} aria-label="กรองตามสถานะใบหน้า" className="[&>select]:h-9 [&>select]:rounded-full">
            <option value="any">ทุกสถานะใบหน้า</option>
            <option value="has_faces">มีใบหน้าที่ระบุไว้</option>
            <option value="no_person">ยังไม่มีชื่อบุคคล</option>
          </Select>

          <label
            className="inline-flex h-9 cursor-pointer items-center gap-2 rounded-full border border-line bg-surface px-3.5 text-sm font-medium text-ink"
            title="นับรูปที่ระบบจับคู่ใบหน้าให้เองด้วย แม้ยังไม่มีคนตรวจ เอาเครื่องหมายออกเพื่อดูเฉพาะรูปที่ยืนยันแล้ว"
          >
            <input type="checkbox" className="size-4 accent-[var(--action)]" checked={filters.includeAi} onChange={(e) => update({ includeAi: e.target.checked })} data-testid="search-include-ai" />
            รวมรูปที่ AI จัดให้
          </label>

          {active && (
            <Button size="sm" variant="ghost" className="h-9" onClick={clearAll} data-testid="search-clear">
              ล้างตัวกรอง
            </Button>
          )}
        </div>
      </div>

      <PhotoResults
        query={query}
        label="ผลการค้นหารูป"
        originLabel="กลับไปผลค้นหา"
        showEvent
        selectionKey={filterIdentity(filters)}
        summary={(meta) => (
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5">
            <p className="text-[16px] font-bold text-ink" aria-live="polite" data-testid="search-total">
              {meta.refreshing && meta.total === 0 ? 'กำลังค้นหา…' : `${active ? 'พบ' : 'รูปทั้งหมดในคลัง'} ${formatCount(meta.total)} รูป`}
            </p>
            {meta.refreshing && <Spinner className="size-4" />}
            <p className="min-w-0 flex-1 text-[13.5px] text-muted" data-testid="search-coverage">
              {[
                filters.personIds.length > 0 ? (filters.includeAi ? 'รวมรูปที่ AI จัดให้ซึ่งยังไม่มีคนตรวจ (มีรูปประกายกำกับ)' : 'นับเฉพาะใบหน้าที่ยืนยันแล้ว') : null,
                // ค้นตามคนขณะที่ยังวิเคราะห์ใบหน้าไม่ครบ: บอกความครอบคลุมภายในขอบเขตที่ค้นอยู่ ไม่ใช่ทั้งคลัง
                filters.personIds.length > 0 && meta.coverage && meta.coverage.facesPending > 0
                  ? `วิเคราะห์ใบหน้าแล้ว ${formatCount(meta.coverage.facesAnalyzed)} จาก ${formatCount(meta.coverage.eligiblePhotos)} รูปในขอบเขตนี้ ผลอาจยังไม่ครบ`
                  : null,
                meta.coverage && meta.coverage.thumbnailsPending > 0
                  ? `ข้อมูลรูปพร้อม ${formatCount(meta.coverage.thumbnailsReady)} จาก ${formatCount(meta.coverage.eligiblePhotos)} รูปในขอบเขตนี้ ผลอาจยังไม่ครบ`
                  : null,
              ]
                .filter(Boolean)
                .join(' · ')}
            </p>
            <label className="flex items-center gap-2 text-sm text-muted">
              เรียงตาม
              <Select value={filters.sort} onChange={(e) => update({ sort: e.target.value as SearchParams['sort'] })} aria-label="เรียงผลตาม">
                <option value="taken_desc">วันถ่าย ใหม่ → เก่า</option>
                <option value="taken_asc">วันถ่าย เก่า → ใหม่</option>
                <option value="added_desc">เพิ่มล่าสุด</option>
                <option value="filename">ชื่อไฟล์</option>
              </Select>
            </label>
          </div>
        )}
        empty={
          active ? (
            <EmptyState
              icon={<SearchX aria-hidden />}
              title="ไม่พบรูปตามเงื่อนไขนี้"
              description={
                [
                  filters.text.trim() ? `คำค้น “${filters.text.trim()}”` : null,
                  filters.personIds.length ? `${filters.personIds.length} บุคคล (${filters.personIds.length > 1 ? (filters.personMode === 'all' ? 'ต้องมีทุกคนในภาพเดียวกัน' : 'มีคนใดคนหนึ่ง') : 'ยืนยันแล้ว'})` : null,
                  filters.eventIds.length ? `${filters.eventIds.length} กิจกรรม` : null,
                  dateActive ? dateLabel : null,
                  filters.faceFilter !== 'any' ? 'สถานะใบหน้าที่เลือก' : null,
                ]
                  .filter(Boolean)
                  .join(' · ') || undefined
              }
              action={
                <>
                  {filters.personIds.length > 1 && filters.personMode === 'all' && (
                    <Button variant="outline" onClick={() => update({ personMode: 'any' })} data-testid="search-switch-any">
                      เปลี่ยนเป็น “มีคนใดคนหนึ่ง”
                    </Button>
                  )}
                  <Button onClick={clearAll}>ล้างตัวกรอง</Button>
                </>
              }
            />
          ) : (
            <EmptyState icon={<Search aria-hidden />} title="ยังไม่มีรูปในคลัง" description="เพิ่มโฟลเดอร์กิจกรรมก่อน แล้วค้นรูปจากชื่อคน กิจกรรม หรือชื่อไฟล์ได้ที่นี่" />
          )
        }
      />
    </Page>
  );
}

function EventFilter({ selected, onChange }: { selected: string[]; onChange: (ids: string[]) => void }) {
  const [text, setText] = useState('');
  const events = useEvents('');
  const term = text.trim().toLocaleLowerCase();
  const list = (events.data ?? []).filter((event) => !term || event.title.toLocaleLowerCase().includes(term));
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button size="sm" icon={<Folder className="size-4" aria-hidden />} className="h-9 rounded-full" data-testid="search-add-event">
          {selected.length ? 'เพิ่มกิจกรรม' : 'กิจกรรม'}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[320px]" aria-label="กรองตามกิจกรรม">
        <div className="border-b border-line p-2">
          <TextInput autoFocus aria-label="ค้นชื่อกิจกรรม" value={text} onChange={(e) => setText(e.target.value)} placeholder="ค้นชื่อกิจกรรม" className="h-9" />
        </div>
        {events.isError ? (
          <ErrorState compact error={events.error} onRetry={() => void events.refetch()} />
        ) : (
          <ul className="scroll-thin max-h-[280px] overflow-y-auto p-1.5">
            {list.length === 0 && <li className="px-3 py-4 text-center text-[13.5px] text-muted">{events.isPending ? 'กำลังโหลด…' : 'ไม่พบกิจกรรม'}</li>}
            {list.map((event) => {
              const checked = selected.includes(event.id);
              return (
                <li key={event.id}>
                  <label className="flex cursor-pointer items-center gap-2.5 rounded-lg px-2 py-1.5 hover:bg-sunken">
                    <input type="checkbox" className="size-4 shrink-0 accent-[var(--action)]" checked={checked} onChange={() => onChange(checked ? selected.filter((id) => id !== event.id) : [...selected, event.id])} />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-semibold text-ink">{event.title}</span>
                      <span className="block text-[12.5px] text-muted">
                        {formatThaiDate(event.eventDate) ?? 'ยังไม่ระบุวันที่'} · {formatCount(event.photoCount)} รูป
                      </span>
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        )}
        <p className="border-t border-line px-3 py-2 text-[12.5px] text-muted">เลือกหลายกิจกรรม = รูปจากงานใดงานหนึ่งที่เลือก</p>
      </PopoverContent>
    </Popover>
  );
}
