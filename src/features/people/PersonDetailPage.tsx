import { useEffect, useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Link, useParams, useSearchParams } from 'react-router';
import { Archive, ArchiveRestore, Calendar, CircleCheck, EllipsisVertical, Image as ImageIcon, Merge, Pencil, Pin, SlidersHorizontal, Sparkles, Undo2, UserRoundSearch, Users, UserX } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { Button, IconButton } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { EmptyState, ErrorState, InlineNotice, LoadingBlock, StatusBadge } from '@/components/ui/Feedback';
import { Select, Tabs } from '@/components/ui/Form';
import { Menu, MenuContent, MenuItem, MenuTrigger } from '@/components/ui/Overlay';
import { EditFilterSelect, EditProgress, progressPhrase } from '@/features/photos/EditProgress';
import { FlagFilterSelect, FlagSummary } from '@/features/photos/PhotoFlags';
import { PhotoResults } from '@/features/photos/PhotoResults';
import { parseEditFilter, parseFlagFilter } from '@/features/search/searchParams';
import { errorMessage, formatCount, formatThaiDate, isErrorCode } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { EditFilter, FlagFilter, PersonDetail, PhotoResult, SearchParams } from '@/lib/ipc/types';
import { invalidatePeopleData, PHOTO_PAGE_SIZE, qk, usePeople, type PhotoQuery } from '@/lib/queries';
import { CoappearanceList } from './CoappearanceList';
import { FaceAvatar } from './FaceAvatar';
import { PersonEditorDialog } from './PersonEditorDialog';
import { MergePeopleDialog } from './MergePeopleDialog';
import { useSetArchived, useTogglePin } from './usePersonActions';

type PersonTab = 'photos' | 'events' | 'together' | 'info';

/** S09 — รายละเอียดบุคคล: รูปของคนหนึ่งข้ามทุกกิจกรรม */
export function PersonDetailPage() {
  const { personId = '' } = useParams();
  const [params, setParams] = useSearchParams();
  const qc = useQueryClient();
  const person = useQuery({ queryKey: qk.person(personId), queryFn: () => call('get_person', { personId }) });
  const togglePin = useTogglePin();
  const setArchived = useSetArchived();
  const [editing, setEditing] = useState(false);
  const [confirmArchive, setConfirmArchive] = useState(false);
  const [merging, setMerging] = useState<'pick' | 'same' | null>(null);
  // "แต่งรูปของคนนี้": เปิดรูปแรกที่ยังไม่แต่งในหน้าแต่งภาพเมื่อรายการพร้อม (DECISIONS ข้อ 32)
  const [autoEdit, setAutoEdit] = useState(false);
  useDocumentTitle(person.data?.displayName);
  // คนอื่นที่ชื่อเหมือนกันทุกตัวอักษร: มักเป็นคนเดียวกันที่ถูกเพิ่มซ้ำ (เช่น ข้อมูลจากรุ่นเดิม) — เสนอให้รวม
  const name = person.data?.displayName.trim() ?? '';
  const namesakes = usePeople({ text: name, includeArchived: true }, Boolean(name)).data?.filter((p) => p.id !== personId && p.displayName.trim() === name) ?? [];

  // เปิดหน้านี้โดยเจตนา → บันทึก "เปิดล่าสุด" หนึ่งครั้งต่อคน
  const recorded = useRef<string | null>(null);
  useEffect(() => {
    if (!person.data || recorded.current === personId) return;
    recorded.current = personId;
    void call('record_person_view', { personId })
      .then(() => qc.invalidateQueries({ queryKey: qk.homePeople }))
      .catch(() => {});
  }, [person.data, personId, qc]);

  const tab = (params.get('tab') as PersonTab | null) ?? 'photos';
  const setTab = (value: PersonTab) => {
    const next = new URLSearchParams(params);
    if (value === 'photos') next.delete('tab');
    else next.set('tab', value);
    setParams(next, { replace: true });
  };
  const editFilter = parseEditFilter(params.get('ed'));
  const flagFilter = parseFlagFilter(params.get('fl'));
  // ตัวกรองการแต่ง (ed) และการคัด (fl) อยู่ใน URL — ล้างพร้อมกันได้ในครั้งเดียว
  const setProgressFilters = (patch: { ed?: EditFilter; fl?: FlagFilter }) => {
    const next = new URLSearchParams(params);
    for (const [key, value] of Object.entries(patch)) {
      if (value === 'any') next.delete(key);
      else next.set(key, value);
    }
    setParams(next, { replace: true });
  };
  const editThisPerson = () => {
    // แท็บรูป + เฉพาะที่ยังไม่แต่ง (คงกิจกรรมที่เลือกไว้) แล้วเปิดรูปแรกเมื่อรายการใหม่พร้อม
    const next = new URLSearchParams(params);
    next.delete('tab');
    next.set('ed', 'unedited');
    setParams(next, { replace: true });
    setAutoEdit(true);
  };

  if (person.isPending) {
    return (
      <Page>
        <PageHeader breadcrumb={[{ label: 'บุคคล', to: '/people' }, { label: 'กำลังโหลด…' }]} />
        <LoadingBlock label="กำลังโหลดข้อมูลบุคคล…" />
      </Page>
    );
  }
  if (person.isError) {
    return (
      <Page>
        <PageHeader breadcrumb={[{ label: 'บุคคล', to: '/people' }, { label: 'เปิดข้อมูลบุคคลไม่ได้' }]} />
        <ErrorState
          error={person.error}
          title={isErrorCode(person.error, 'NOT_FOUND') ? 'ไม่พบบุคคลนี้ในคลัง' : 'โหลดข้อมูลบุคคลไม่สำเร็จ'}
          onRetry={() => void person.refetch()}
          extraAction={
            <Link to="/people" className="inline-flex h-8 items-center rounded-lg border border-line-strong bg-surface px-3 text-[13px] font-semibold text-ink hover:bg-sunken">
              กลับไปรายชื่อบุคคล
            </Link>
          }
        />
      </Page>
    );
  }

  const data = person.data;
  return (
    <Page>
      <PageHeader breadcrumb={[{ label: 'บุคคล', to: '/people' }, { label: data.displayName }]} />

      <div className="mb-5 flex flex-wrap items-start gap-5">
        <FaceAvatar asset={data.coverAsset} name={data.displayName} shape="rounded" className="size-[128px] rounded-2xl text-5xl" />
        <div className="min-w-[240px] flex-1">
          <div className="flex flex-wrap items-center gap-2.5">
            <h1 className="text-[30px] leading-tight font-extrabold tracking-tight text-ink" data-testid="person-name">
              {data.displayName}
            </h1>
            {data.hasConfirmedFaces && (
              <StatusBadge tone="ok" icon={<CircleCheck aria-hidden />}>
                ยืนยันแล้ว
              </StatusBadge>
            )}
            {data.archived && <StatusBadge tone="neutral">เก็บไว้</StatusBadge>}
          </div>
          {(data.nickname || data.aliases.length > 0) && (
            <p className="mt-0.5 text-[15px] text-muted">{[data.nickname, ...data.aliases].filter(Boolean).join(' · ')}</p>
          )}
          <p className="mt-1.5 text-[16px] text-ink" data-testid="person-counts">
            <span className="font-bold">{formatCount(data.confirmedPhotoCount + data.aiPhotoCount)}</span> รูป · <span className="font-bold">{formatCount(data.confirmedEventCount)}</span> กิจกรรม
            <span className="ml-2 text-[14px] text-muted">
              (ยืนยันแล้ว {formatCount(data.confirmedPhotoCount)}
              {data.aiPhotoCount > 0 ? ` · AI จัดให้ ${formatCount(data.aiPhotoCount)}` : ''})
            </span>
          </p>
          {data.groups.length > 0 && (
            <ul className="mt-2 flex flex-wrap gap-1.5" aria-label="กลุ่ม">
              {data.groups.map((group) => (
                <li key={group.id} className="rounded-full bg-brand-soft px-3 py-0.5 text-[13px] font-semibold text-action">
                  {group.name}
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2.5">
          <Button
            icon={<Pin className="size-[18px]" fill={data.pinned ? 'currentColor' : 'none'} aria-hidden />}
            variant={data.pinned ? 'outline' : 'secondary'}
            aria-pressed={data.pinned}
            busy={togglePin.isPending}
            onClick={() => togglePin.mutate(data)}
            data-testid="person-detail-pin"
          >
            {data.pinned ? 'ปักหมุดอยู่' : 'ปักหมุด'}
          </Button>
          <Button
            icon={<SlidersHorizontal className="size-4" aria-hidden />}
            onClick={editThisPerson}
            title="เปิดรูปของคนนี้ที่ยังไม่แต่งทีละรูป บันทึกแล้วไปรูปถัดไปได้เลย"
            data-testid="person-edit-photos"
          >
            แต่งรูปของคนนี้
          </Button>
          <Button icon={<Pencil className="size-4" aria-hidden />} onClick={() => setEditing(true)} data-testid="person-edit">
            แก้ไขข้อมูล
          </Button>
          <Menu>
            <MenuTrigger asChild>
              <IconButton label="คำสั่งเพิ่มเติมของบุคคลนี้" className="border border-line-strong bg-surface shadow-card">
                <EllipsisVertical className="size-[18px]" aria-hidden />
              </IconButton>
            </MenuTrigger>
            <MenuContent>
              {data.archived ? (
                <MenuItem icon={<ArchiveRestore />} onSelect={() => setArchived.mutate({ personId: data.id, archived: false })}>
                  นำกลับมาในรายชื่อหลัก
                </MenuItem>
              ) : (
                <MenuItem icon={<Archive />} onSelect={() => setConfirmArchive(true)}>
                  เก็บออกจากรายชื่อหลัก
                </MenuItem>
              )}
              <MenuItem icon={<Merge />} onSelect={() => setMerging('pick')}>
                รวมกับบุคคลที่ซ้ำ…
              </MenuItem>
            </MenuContent>
          </Menu>
        </div>
      </div>

      {namesakes.length > 0 && (
        <InlineNotice
          tone="brand"
          icon={<Users aria-hidden />}
          className="mb-5 items-center"
          action={
            <Button size="sm" variant="primary" icon={<Merge className="size-4" aria-hidden />} onClick={() => setMerging('same')} data-testid="person-namesakes-merge">
              รวมเป็นคนเดียว
            </Button>
          }
        >
          <span data-testid="person-namesakes">
            มีบุคคลชื่อ “{data.displayName}” อีก {formatCount(namesakes.length)} คน ({namesakes.map((p) => `${formatCount(p.confirmedPhotoCount + p.aiPhotoCount)} รูป`).join(', ')}) — ถ้าเป็นคนเดียวกัน รวมไว้ที่นี่ได้
          </span>
        </InlineNotice>
      )}

      <Tabs<PersonTab>
        label="ส่วนของบุคคล"
        testId="person-tab"
        value={tab}
        onChange={setTab}
        items={[
          { value: 'photos', label: 'รูปทั้งหมด' },
          { value: 'events', label: `กิจกรรม${data.events.length ? ` (${formatCount(data.events.length)})` : ''}` },
          { value: 'together', label: `ปรากฏร่วมในภาพ${data.coappearances.length ? ` (${formatCount(data.coappearances.length)})` : ''}` },
          { value: 'info', label: 'ข้อมูล' },
        ]}
        className="mb-4"
      />

      {tab === 'photos' && (
        <PersonPhotos
          person={data}
          eventId={params.get('event')}
          onEventChange={(id) => {
            const next = new URLSearchParams(params);
            if (id) next.set('event', id);
            else next.delete('event');
            setParams(next, { replace: true });
          }}
          editFilter={editFilter}
          flagFilter={flagFilter}
          onFiltersChange={setProgressFilters}
          // เริ่มเมื่อ URL เปลี่ยนเป็น "ยังไม่แต่ง" แล้วจริง (react-router เปลี่ยน URL แบบ transition ช้ากว่า state นี้)
          autoEdit={autoEdit && editFilter === 'unedited'}
          onAutoEditHandled={(opened) => {
            setAutoEdit(false);
            if (!opened) toast.success(`รูปของ ${data.displayName}${params.get('event') ? 'ในกิจกรรมที่เลือก' : ''} แต่งครบทุกรูปแล้ว`);
          }}
        />
      )}
      {tab === 'events' && <PersonEvents person={data} />}
      {tab === 'together' && (
        <div className="max-w-[560px] rounded-card border border-line bg-surface p-5 shadow-card">
          <p className="mb-3 text-sm text-muted">เลือกคนเพื่อค้นรูปที่อยู่ด้วยกัน · นับจากทุกกิจกรรม เฉพาะใบหน้าที่ยืนยันแล้ว (เป็นการปรากฏในภาพเดียวกัน ไม่ใช่ข้อสรุปความสัมพันธ์)</p>
          <CoappearanceList personId={data.id} items={data.coappearances} layout="list" />
        </div>
      )}
      {tab === 'info' && <PersonInfo person={data} onEdit={() => setEditing(true)} />}

      <PersonEditorDialog open={editing} onOpenChange={setEditing} personId={data.id} />
      <MergePeopleDialog open={merging !== null} onOpenChange={(open) => !open && setMerging(null)} target={data} initial={merging === 'same' ? namesakes : []} />
      <ConfirmDialog
        open={confirmArchive}
        onOpenChange={setConfirmArchive}
        title={`เก็บ ${data.displayName} ออกจากรายชื่อหลัก?`}
        description="เก็บบุคคลนี้ออกจากรายชื่อหลัก โดยคงข้อมูลในภาพไว้ นำกลับมาได้จากตัวกรอง “รวมคนที่เก็บไว้” ในหน้าบุคคล"
        confirmLabel="เก็บออกจากรายชื่อหลัก"
        busy={setArchived.isPending}
        onConfirm={() => setArchived.mutate({ personId: data.id, archived: true }, { onSettled: () => setConfirmArchive(false) })}
      />
    </Page>
  );
}

function PersonPhotos({
  person,
  eventId,
  onEventChange,
  editFilter,
  flagFilter,
  onFiltersChange,
  autoEdit,
  onAutoEditHandled,
}: {
  person: PersonDetail;
  eventId: string | null;
  onEventChange: (id: string | null) => void;
  editFilter: EditFilter;
  flagFilter: FlagFilter;
  onFiltersChange: (patch: { ed?: EditFilter; fl?: FlagFilter }) => void;
  autoEdit: boolean;
  onAutoEditHandled: (opened: boolean) => void;
}) {
  const [sort, setSort] = useState<SearchParams['sort']>('taken_desc');
  // ค่าเริ่มต้น: รวมรูปที่ AI จัดให้ ผู้ใช้ไล่ดูแล้วแจ้งเฉพาะรูปที่ไม่ใช่
  const [chosen, setScope] = useState<PhotoScope>('all');
  // "แต่งรูปของคนนี้" ขณะดูรูปที่แจ้งว่าไม่ใช่: กลับไปรูปของคนนี้ก่อน (ไม่แต่งรูปของคนอื่น)
  useEffect(() => {
    if (autoEdit && chosen === 'rejected') setScope('all');
  }, [autoEdit, chosen]);
  // รูปที่แจ้งว่าไม่ใช่ถูกคืนจนหมด: กลับไปมุมมองปกติ
  const scope: PhotoScope = chosen === 'rejected' && person.rejectedPhotoCount === 0 ? 'all' : chosen;
  const query = useMemo<PhotoQuery>(() => {
    const base = { personMode: 'all' as const, eventIds: eventId ? [eventId] : [], faceFilter: 'any' as const, sort, limit: PHOTO_PAGE_SIZE, editFilter, flagFilter };
    return scope === 'rejected' ? { ...base, personIds: [], rejectedPersonId: person.id } : { ...base, personIds: [person.id], includeSuggested: scope === 'all' };
  }, [person.id, eventId, sort, scope, editFilter, flagFilter]);
  return (
    <div>
      <div className="mb-3.5 flex flex-wrap items-center gap-2.5">
        <Select value={eventId ?? ''} onChange={(e) => onEventChange(e.target.value || null)} aria-label="กรองตามกิจกรรม" className="max-w-[320px]">
          <option value="">ทุกกิจกรรม</option>
          {person.events.map((event) => (
            <option key={event.eventId} value={event.eventId}>
              {event.title} ({formatCount(event.photoCount)})
            </option>
          ))}
        </Select>
        <Select value={sort} onChange={(e) => setSort(e.target.value as SearchParams['sort'])} aria-label="เรียงรูปตาม">
          <option value="taken_desc">วันถ่าย ใหม่ → เก่า</option>
          <option value="taken_asc">วันถ่าย เก่า → ใหม่</option>
          <option value="added_desc">เพิ่มล่าสุด</option>
          <option value="filename">ชื่อไฟล์</option>
        </Select>
        <EditFilterSelect value={editFilter} onChange={(ed) => onFiltersChange({ ed })} />
        <FlagFilterSelect value={flagFilter} onChange={(fl) => onFiltersChange({ fl })} />
        {(person.aiPhotoCount > 0 || person.rejectedPhotoCount > 0) && (
          <Tabs<PhotoScope>
            label="ขอบเขตของรูป"
            variant="pill"
            value={scope}
            onChange={setScope}
            items={[
              { value: 'all', label: 'ทั้งหมด' },
              { value: 'confirmed', label: 'ยืนยันแล้วเท่านั้น' },
              ...(person.rejectedPhotoCount > 0 ? [{ value: 'rejected' as const, label: `ที่แจ้งว่าไม่ใช่ (${formatCount(person.rejectedPhotoCount)})` }] : []),
            ]}
            testId="person-photo-scope"
          />
        )}
      </div>
      {scope === 'rejected' && (
        <InlineNotice tone="info" icon={<Undo2 aria-hidden />} className="mb-3.5">
          รูปที่เคยแจ้งว่าไม่ใช่ {person.displayName} ถ้าแจ้งผิด ให้ติ๊กเลือกแล้วกด “คืนให้ {person.displayName}” รูปจะกลับไปเป็นรูปที่ AI จัดให้
        </InlineNotice>
      )}
      {person.aiPhotoCount > 0 && scope === 'all' && (
        <InlineNotice tone="info" icon={<Sparkles aria-hidden />} className="mb-3.5">
          รูปที่มีรูปประกาย <Sparkles className="inline size-3.5 align-[-2px]" aria-hidden /> คือรูปที่ AI จัดให้ ยังไม่มีคนตรวจ ถ้าเจอรูปที่ไม่ใช่ ให้ติ๊กเลือกแล้วกด “ไม่ใช่ {person.displayName}”
        </InlineNotice>
      )}
      <PhotoResults
        query={query}
        label={scope === 'rejected' ? `รูปที่แจ้งว่าไม่ใช่ ${person.displayName}` : `รูปของ ${person.displayName}`}
        originLabel="กลับไปบุคคล"
        showEvent={!eventId}
        selectionKey={`${person.id}|${eventId ?? ''}|${scope}|${editFilter}|${flagFilter}`}
        autoEdit={autoEdit && scope !== 'rejected'}
        onAutoEditHandled={onAutoEditHandled}
        selectionActions={({ photos, clear }) =>
          scope === 'rejected' ? <GiveBack person={person} photos={photos} onDone={clear} /> : <NotThisPerson person={person} photos={photos} onDone={clear} />
        }
        summary={(meta) =>
          meta.total > 0 || (meta.coverage?.eligiblePhotos ?? 0) > 0 ? (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
              <p className="text-sm text-muted" aria-live="polite">
                {formatCount(meta.total)} รูป{progressPhrase(editFilter, flagFilter)}
                {eventId ? ' ในกิจกรรมที่เลือก' : ' จากทุกกิจกรรม'}
              </p>
              {scope !== 'rejected' && <EditProgress meta={meta} />}
              <FlagSummary coverage={meta.coverage} />
            </div>
          ) : null
        }
        empty={
          editFilter === 'unedited' ? (
            <EmptyState
              icon={<CircleCheck aria-hidden />}
              title={`แต่งรูป${flagFilter === 'any' ? '' : progressPhrase('any', flagFilter)}ของ ${person.displayName} ครบแล้ว`}
              description={`ไม่มีรูป${progressPhrase(editFilter, flagFilter)}${eventId ? 'ในกิจกรรมที่เลือก' : ''} — รูปที่แต่งแล้วมีป้าย “แต่งแล้ว” กำกับ`}
              action={<Button onClick={() => onFiltersChange({ ed: 'any', fl: 'any' })}>ดูทุกรูป</Button>}
            />
          ) : editFilter !== 'any' || flagFilter !== 'any' ? (
            <EmptyState
              icon={<SlidersHorizontal aria-hidden />}
              title={`ไม่มีรูป${progressPhrase(editFilter, flagFilter)}`}
              description={
                flagFilter === 'any'
                  ? `กด “แต่งรูปของคนนี้” เพื่อเริ่มแต่งทีละรูป${eventId ? ' (เฉพาะกิจกรรมที่เลือก)' : ''}`
                  : 'คัดรูปในตัวดูภาพด้วยคีย์ P (เลือก) และ X (ไม่เอา) หรือเลือกหลายรูปแล้วใช้เมนู “คัดรูป”'
              }
              action={<Button onClick={() => onFiltersChange({ ed: 'any', fl: 'any' })}>ดูทุกรูป</Button>}
            />
          ) : eventId ? (
            <EmptyState icon={<UserRoundSearch aria-hidden />} title="ไม่มีรูปของบุคคลนี้ในกิจกรรมที่เลือก" action={<Button onClick={() => onEventChange(null)}>ดูทุกกิจกรรม</Button>} />
          ) : (
            <EmptyState
              icon={<UserRoundSearch aria-hidden />}
              title="ยังไม่มีรูปที่เชื่อมกับบุคคลนี้"
              description="เปิดรูปที่มีบุคคลนี้ กด “เพิ่มกรอบใบหน้า” แล้วเลือกชื่อนี้ รูปจะปรากฏที่นี่และค้นจากชื่อได้ทันที"
              action={
                <Link to="/events" className="inline-flex h-10 items-center rounded-control bg-action px-4 text-sm font-semibold text-on-action hover:bg-action-hover">
                  เลือกใบหน้าจากรูปในกิจกรรม
                </Link>
              }
            />
          )
        }
      />
    </div>
  );
}

type PhotoScope = 'all' | 'confirmed' | 'rejected';

function useRestoreSuggestions(person: PersonDetail) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (faceIds: string[]) => call('restore_suggestions', { faceIds, personId: person.id }),
    onSuccess: (_result, faceIds) => {
      invalidatePeopleData(qc);
      toast.success(`คืน ${formatCount(faceIds.length)} รูปให้ ${person.displayName} แล้ว`);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** คืนรูปที่เคยแจ้งว่าไม่ใช่ (แจ้งผิด): กลับไปเป็นรูปที่ AI จัดให้ */
function GiveBack({ person, photos, onDone }: { person: PersonDetail; photos: PhotoResult[] | null; onDone: () => void }) {
  const restore = useRestoreSuggestions(person);
  const faceIds = (photos ?? []).flatMap((photo) => photo.rejectedFaces);
  return (
    <Button
      size="sm"
      variant="outline"
      icon={<Undo2 className="size-4" aria-hidden />}
      disabled={faceIds.length === 0}
      busy={restore.isPending}
      title={photos === null ? 'ใช้ได้กับรูปที่ติ๊กเลือกเอง ไม่ใช่ “ผลทั้งหมด”' : undefined}
      onClick={() => restore.mutate(faceIds, { onSuccess: onDone })}
      data-testid="restore-selected"
    >
      คืนให้ {person.displayName}
    </Button>
  );
}

/** "ไม่ใช่คนนี้" กับรูปที่เลือก: ใช้ได้กับรูปที่ AI จัดให้ (รูปที่ยืนยันแล้วแก้รายใบหน้าในตัวดูภาพ) */
function NotThisPerson({ person, photos, onDone }: { person: PersonDetail; photos: PhotoResult[] | null; onDone: () => void }) {
  const qc = useQueryClient();
  const restore = useRestoreSuggestions(person);
  const faceIds = (photos ?? []).flatMap((photo) => photo.suggestedPeople.filter((s) => s.person.id === person.id).map((s) => s.faceId));
  const reject = useMutation({
    mutationFn: () => call('reject_suggestions', { faceIds, personId: person.id }),
    onSuccess: () => {
      invalidatePeopleData(qc);
      onDone();
      const rejected = faceIds;
      toast.success(`เอา ${formatCount(rejected.length)} รูปออกจากรูปของ ${person.displayName} แล้ว ระบบจะไม่จัดรูปเหล่านี้ให้บุคคลนี้อีก`, {
        action: { label: 'เลิกทำ', onClick: () => restore.mutate(rejected) },
      });
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
  const reason =
    photos === null
      ? 'ใช้ได้กับรูปที่ติ๊กเลือกเอง ไม่ใช่ “ผลทั้งหมด”'
      : faceIds.length === 0
        ? 'รูปที่เลือกเป็นรูปที่ยืนยันแล้ว แก้ได้ในตัวดูภาพโดยคลิกกรอบใบหน้า'
        : `แจ้งว่า ${formatCount(faceIds.length)} รูปที่ AI จัดให้นี้ไม่ใช่ ${person.displayName}`;
  return (
    <Button size="sm" variant="outline" icon={<UserX className="size-4" aria-hidden />} disabled={faceIds.length === 0} busy={reject.isPending} title={reason} onClick={() => reject.mutate()} data-testid="reject-selected">
      ไม่ใช่ {person.displayName}
    </Button>
  );
}

function PersonEvents({ person }: { person: PersonDetail }) {
  if (person.events.length === 0) {
    return <EmptyState icon={<Calendar aria-hidden />} title="ยังไม่พบบุคคลนี้ในกิจกรรมใด" description="เมื่อยืนยันใบหน้าของบุคคลนี้ในรูป กิจกรรมของรูปนั้นจะแสดงที่นี่" />;
  }
  return (
    <ul className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-4">
      {person.events.map((event) => (
        <li key={event.eventId}>
          {/* เปิดกิจกรรมพร้อมตัวกรองคนเดิม */}
          <Link
            to={`/events/${event.eventId}?person=${person.id}`}
            className="group block overflow-hidden rounded-card border border-line bg-surface shadow-card hover:shadow-raised"
            data-testid="person-event-card"
          >
            <div className="aspect-[16/9.5] bg-sunken">
              {event.coverPhotoId ? (
                <img src={assetUrl('thumb', event.coverPhotoId)} alt="" loading="lazy" className="size-full object-cover" />
              ) : (
                <div className="flex size-full items-center justify-center text-faint">
                  <ImageIcon className="size-8" aria-hidden />
                </div>
              )}
            </div>
            <div className="px-4 py-3">
              <h3 className="truncate text-[16px] font-bold text-ink">{event.title}</h3>
              <p className="text-[13.5px] text-muted">
                {formatThaiDate(event.eventDate) ?? 'ยังไม่ระบุวันที่'} · มีบุคคลนี้ {formatCount(event.photoCount)} รูป
              </p>
            </div>
          </Link>
        </li>
      ))}
    </ul>
  );
}

function PersonInfo({ person, onEdit }: { person: PersonDetail; onEdit: () => void }) {
  const row = 'grid grid-cols-[140px_1fr] gap-3 border-b border-line py-3 text-sm last:border-b-0';
  return (
    <div className="max-w-[680px] rounded-card border border-line bg-surface px-6 py-2 shadow-card">
      <dl>
        <div className={row}>
          <dt className="font-semibold text-ink">ชื่อที่ใช้แสดง</dt>
          <dd className="text-ink">{person.displayName}</dd>
        </div>
        <div className={row}>
          <dt className="font-semibold text-ink">ชื่อเล่น / ชื่อรอง</dt>
          <dd className={person.nickname ? 'text-ink' : 'text-muted'}>{person.nickname ?? 'ไม่มี'}</dd>
        </div>
        <div className={row}>
          <dt className="font-semibold text-ink">ชื่ออื่นที่ใช้ค้นหา</dt>
          <dd className={person.aliases.length ? 'text-ink' : 'text-muted'}>{person.aliases.length ? person.aliases.join(' · ') : 'ไม่มี'}</dd>
        </div>
        <div className={row}>
          <dt className="font-semibold text-ink">กลุ่ม</dt>
          <dd className={person.groups.length ? 'text-ink' : 'text-muted'}>{person.groups.length ? person.groups.map((g) => g.name).join(' · ') : 'ยังไม่ได้จัดกลุ่ม'}</dd>
        </div>
        <div className={row}>
          <dt className="font-semibold text-ink">หมายเหตุ</dt>
          <dd className={person.notes ? 'break-words whitespace-pre-wrap text-ink' : 'text-muted'}>{person.notes || 'ไม่มี'}</dd>
        </div>
      </dl>
      {!person.hasConfirmedFaces && (
        <InlineNotice tone="info" className="my-3">
          บุคคลนี้ยังไม่มีใบหน้าที่ยืนยันในรูปใด จึงยังไม่มีภาพตัวแทน
        </InlineNotice>
      )}
      <div className="flex justify-end py-3">
        <Button icon={<Pencil className="size-4" aria-hidden />} onClick={onEdit}>
          แก้ไขข้อมูล
        </Button>
      </div>
    </div>
  );
}
