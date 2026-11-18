import { useMemo, useRef, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Link, useParams, useSearchParams } from 'react-router';
import { toast } from 'sonner';
import { CircleCheck, CloudOff, Copy, FolderOpen, Image as ImageIcon, ImageOff, RefreshCw, ScanFace, TriangleAlert, UserRoundSearch } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { Button, IconButton } from '@/components/ui/Button';
import { EmptyState, ErrorState, InlineNotice, LoadingBlock, StatusBadge } from '@/components/ui/Feedback';
import { Field, Select, Tabs, TextArea, TextInput } from '@/components/ui/Form';
import { JobCard, JobStrip } from '@/features/jobs/JobCard';
import { FaceAvatar } from '@/features/people/FaceAvatar';
import { PhotoResults } from '@/features/photos/PhotoResults';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount, formatThaiDate, isErrorCode, middleEllipsis, newRequestId } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { EventDetail, SearchParams } from '@/lib/ipc/types';
import { isJobActive, PHOTO_PAGE_SIZE, qk, useActiveJobs, useCapabilities, useEvent, type PhotoQuery } from '@/lib/queries';

type EventTab = 'photos' | 'people' | 'info' | 'history';

/** S13 — รายละเอียดกิจกรรม: รูปและคนในงานเดียว พร้อมความเชื่อมโยงกับโฟลเดอร์จริง */
export function EventDetailPage() {
  const { eventId = '' } = useParams();
  const [params, setParams] = useSearchParams();
  const event = useEvent(eventId);
  const activeJobs = useActiveJobs();
  const capabilities = useCapabilities();
  const qc = useQueryClient();
  useDocumentTitle(event.data?.title);

  const tab = (params.get('tab') as EventTab | null) ?? 'photos';
  const setParam = (key: string, value: string | null) => {
    const next = new URLSearchParams(params);
    if (value === null) next.delete(key);
    else next.set(key, value);
    setParams(next, { replace: true });
  };

  const reveal = useMutation({
    mutationFn: () => call('reveal_event_folder', { eventId }),
    onError: (error) => toast.error(errorMessage(error)),
  });
  // หนึ่ง requestId ต่อการกดหนึ่งครั้ง: กดซ้ำระหว่างรอไม่สร้างงานสำเนา
  const rescanRequest = useRef<string | null>(null);
  const rescan = useMutation({
    mutationFn: () => {
      rescanRequest.current ??= newRequestId();
      return call('rescan_event', { eventId, requestId: rescanRequest.current });
    },
    onSuccess: () => {
      rescanRequest.current = null;
      void qc.invalidateQueries({ queryKey: ['jobs'] });
      void qc.invalidateQueries({ queryKey: ['events'] });
      toast.success('เริ่มตรวจรูปใหม่แล้ว จะเพิ่มเฉพาะรูปที่ยังไม่มีในคลัง ชื่อบุคคลที่ยืนยันไว้ไม่เปลี่ยน');
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  const analyze = useMutation({
    mutationFn: () => call('analyze_faces', { eventId }),
    onSuccess: (result) => {
      void qc.invalidateQueries({ queryKey: ['jobs'] });
      void qc.invalidateQueries({ queryKey: ['events'] });
      toast.success(result.jobIds.length > 0 ? 'เริ่มวิเคราะห์ใบหน้าเบื้องหลังแล้ว ชื่อที่ยืนยันไว้ไม่เปลี่ยน' : 'ไม่มีรูปที่ต้องวิเคราะห์เพิ่มในงานนี้');
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  if (event.isPending) {
    return (
      <Page>
        <PageHeader breadcrumb={[{ label: 'กิจกรรม', to: '/events' }, { label: 'กำลังโหลด…' }]} />
        <LoadingBlock label="กำลังโหลดกิจกรรม…" />
      </Page>
    );
  }
  if (event.isError) {
    return (
      <Page>
        <PageHeader breadcrumb={[{ label: 'กิจกรรม', to: '/events' }, { label: 'เปิดกิจกรรมไม่ได้' }]} />
        <ErrorState
          error={event.error}
          title={isErrorCode(event.error, 'NOT_FOUND') ? 'ไม่พบกิจกรรมนี้ในคลัง' : 'โหลดกิจกรรมไม่สำเร็จ'}
          onRetry={() => void event.refetch()}
          extraAction={
            <Link to="/events" className="inline-flex h-8 items-center rounded-lg border border-line-strong bg-surface px-3 text-[13px] font-semibold text-ink hover:bg-sunken">
              กลับไปรายการกิจกรรม
            </Link>
          }
        />
      </Page>
    );
  }

  const data = event.data;
  const job = (activeJobs.data ?? []).find((j) => j.eventId === data.id && (isJobActive(j) || j.state === 'paused')) ?? data.jobs.find((j) => isJobActive(j) || j.state === 'paused');
  const date = formatThaiDate(data.eventDate);
  const faceReady = capabilities.data?.faceAnalysis.available ?? false;
  const facesComplete = data.faceAnalyzedCount >= data.photoCount;

  return (
    <Page>
      <PageHeader breadcrumb={[{ label: 'กิจกรรม', to: '/events' }, { label: data.title }]} />

      <div className="mb-4 flex flex-wrap items-start gap-5">
        <div className="aspect-[16/10] w-[250px] shrink-0 overflow-hidden rounded-card bg-sunken">
          {data.coverPhotoId ? (
            <img src={assetUrl('thumb', data.coverPhotoId)} alt="" className="size-full object-cover" />
          ) : (
            <div className="flex size-full items-center justify-center text-faint">
              <ImageIcon className="size-9" aria-hidden />
            </div>
          )}
        </div>
        <div className="min-w-[260px] flex-1">
          <h1 className="text-[30px] leading-tight font-extrabold tracking-tight text-ink" data-testid="event-title">
            {data.title}
          </h1>
          <p className="mt-1 text-[16px] text-muted">
            {date ?? 'ยังไม่ระบุวันที่'} · <span data-testid="event-photo-count">{formatCount(data.photoCount)} รูป</span>
          </p>
          <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1.5">
            {data.sourceAvailable ? (
              <StatusBadge tone="ok" icon={<FolderOpen aria-hidden />}>
                ต้นทางพร้อมใช้
              </StatusBadge>
            ) : (
              <StatusBadge tone="warn" icon={<CloudOff aria-hidden />}>
                โฟลเดอร์ไม่พร้อม
              </StatusBadge>
            )}
            <span className="h-4 w-px bg-line-strong" aria-hidden />
            <span className="min-w-0 truncate text-sm text-muted" title={data.folderPath}>
              {middleEllipsis(data.folderPath, 64)}
            </span>
          </div>
          {faceReady && data.photoCount > 0 && (
            <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-sm">
              <span className="inline-flex items-center gap-1.5 text-muted" data-testid="event-face-progress">
                <ScanFace className="size-4" aria-hidden />
                {facesComplete ? 'วิเคราะห์ใบหน้าครบแล้ว' : `วิเคราะห์ใบหน้า ${formatCount(data.faceAnalyzedCount)}/${formatCount(data.photoCount)}`}
              </span>
              {!facesComplete && !job && (
                <Button size="sm" busy={analyze.isPending} disabled={!data.sourceAvailable} onClick={() => analyze.mutate()} data-testid="event-analyze-faces">
                  วิเคราะห์ใบหน้าที่เหลือ
                </Button>
              )}
              <Link to={`/review?event=${data.id}`} className="font-semibold text-action hover:underline" data-testid="event-review-link">
                ตรวจสอบใบหน้าของงานนี้
              </Link>
            </div>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2.5 self-center">
          <Button icon={<FolderOpen className="size-[18px]" aria-hidden />} onClick={() => reveal.mutate()} disabled={!data.sourceAvailable} busy={reveal.isPending} data-testid="event-reveal">
            เปิดใน Finder
          </Button>
          <Button
            variant="primary"
            icon={<RefreshCw className="size-[18px]" aria-hidden />}
            onClick={() => rescan.mutate()}
            busy={rescan.isPending}
            disabled={!data.sourceAvailable || Boolean(job)}
            title={job ? 'มีงานของกิจกรรมนี้กำลังทำอยู่' : undefined}
            data-testid="event-rescan"
          >
            ตรวจรูปใหม่
          </Button>
        </div>
      </div>

      {!data.sourceAvailable && (
        <InlineNotice tone="warn" icon={<CloudOff aria-hidden />} className="mb-4">
          โฟลเดอร์ต้นทางไม่พร้อมใช้งาน เชื่อมต่อไดรฟ์แล้วเปิดกิจกรรมนี้อีกครั้ง ระหว่างนี้ยังดูภาพย่อและชื่อบุคคลที่บันทึกไว้ได้ แต่เปิดต้นฉบับ ส่งออก และตรวจรูปใหม่ไม่ได้
        </InlineNotice>
      )}

      {job && (
        <div className="mb-4">
          <JobStrip job={job} />
        </div>
      )}

      <Tabs<EventTab>
        label="ส่วนของกิจกรรม"
        testId="event-tab"
        value={tab}
        onChange={(value) => setParam('tab', value === 'photos' ? null : value)}
        items={[
          { value: 'photos', label: 'รูปภาพ' },
          { value: 'people', label: `บุคคลที่พบ${data.people.length ? ` (${formatCount(data.people.length)})` : ''}` },
          { value: 'info', label: 'ข้อมูลกิจกรรม' },
          { value: 'history', label: 'ประวัติการประมวลผล' },
        ]}
        className="mb-4"
      />

      {tab === 'photos' && <PhotosTab event={data} personId={params.get('person')} onPersonChange={(id) => setParam('person', id)} scanning={Boolean(job)} />}
      {tab === 'people' && <PeopleTab event={data} onShowPhotos={(id) => setParams(new URLSearchParams({ person: id }), { replace: true })} />}
      {tab === 'info' && <InfoTab key={data.recordVersion} event={data} />}
      {tab === 'history' && <HistoryTab event={data} />}
    </Page>
  );
}

function PhotosTab({ event, personId, onPersonChange, scanning }: { event: EventDetail; personId: string | null; onPersonChange: (id: string | null) => void; scanning: boolean }) {
  const [faceFilter, setFaceFilter] = useState<SearchParams['faceFilter']>('any');
  const [sort, setSort] = useState<SearchParams['sort']>('taken_asc');
  // คน filter ที่มาจากหน้าบุคคลอาจยังไม่อยู่ในรายชื่อของงาน (เช่นถูกเอาออกแล้ว) ก็ยังแสดง chip ให้ล้างได้
  const selectedPerson = event.people.find((p) => p.person.id === personId)?.person ?? null;
  const query = useMemo<PhotoQuery>(
    () => ({
      eventIds: [event.id],
      personIds: personId ? [personId] : [],
      personMode: 'all',
      faceFilter,
      sort,
      limit: PHOTO_PAGE_SIZE,
    }),
    [event.id, personId, faceFilter, sort],
  );

  return (
    <div>
      <div className="mb-3.5 flex flex-wrap items-center gap-2">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2" role="group" aria-label="กรองตามบุคคลในงานนี้">
          <PersonChip active={!personId} onClick={() => onPersonChange(null)} label="ทุกคน" testId="event-person-all" />
          {event.people.slice(0, 12).map((item) => (
            <PersonChip
              key={item.person.id}
              active={personId === item.person.id}
              onClick={() => onPersonChange(personId === item.person.id ? null : item.person.id)}
              label={item.person.displayName}
              count={item.photoCount}
              asset={item.person.coverAsset}
              testId="event-person-chip"
            />
          ))}
          {personId && !selectedPerson && <PersonChip active onClick={() => onPersonChange(null)} label="บุคคลที่เลือกไว้ (กดเพื่อล้าง)" testId="event-person-chip" />}
        </div>
        <Select value={faceFilter} onChange={(e) => setFaceFilter(e.target.value as SearchParams['faceFilter'])} aria-label="กรองตามสถานะใบหน้า">
          <option value="any">ทุกสถานะใบหน้า</option>
          <option value="has_faces">มีใบหน้าที่ระบุไว้</option>
          <option value="no_person">ยังไม่มีชื่อบุคคล</option>
        </Select>
        <Select value={sort} onChange={(e) => setSort(e.target.value as SearchParams['sort'])} aria-label="เรียงรูปตาม">
          <option value="taken_asc">วันถ่าย เก่า → ใหม่</option>
          <option value="taken_desc">วันถ่าย ใหม่ → เก่า</option>
          <option value="filename">ชื่อไฟล์</option>
          <option value="added_desc">เพิ่มล่าสุด</option>
        </Select>
      </div>
      <PhotoResults
        query={query}
        label={`รูปในกิจกรรม ${event.title}`}
        originLabel="กลับไปกิจกรรม"
        selectionKey={`${event.id}|${personId ?? ''}|${faceFilter}`}
        summary={(meta) =>
          meta.total > 0 && (personId || faceFilter !== 'any') ? (
            <p className="mb-2.5 text-sm text-muted" aria-live="polite">
              พบ {formatCount(meta.total)} รูปจาก {formatCount(event.photoCount)} รูปในงานนี้
              {selectedPerson ? ` ที่ยืนยันแล้วว่ามี ${selectedPerson.displayName}` : ''}
            </p>
          ) : null
        }
        empty={
          personId || faceFilter !== 'any' ? (
            <EmptyState
              icon={<UserRoundSearch aria-hidden />}
              title="ไม่มีรูปที่ตรงกับตัวกรองนี้ในงานนี้"
              description="ผลนี้นับเฉพาะใบหน้าที่ยืนยันแล้ว ลองล้างตัวกรองเพื่อดูรูปทั้งหมดของกิจกรรม"
              action={
                <Button
                  onClick={() => {
                    onPersonChange(null);
                    setFaceFilter('any');
                  }}
                >
                  ล้างตัวกรอง
                </Button>
              }
            />
          ) : scanning ? (
            <LoadingBlock label="กำลังตรวจรายการไฟล์ในโฟลเดอร์ รูปจะปรากฏที่นี่เมื่อพบ…" />
          ) : (
            <EmptyState icon={<ImageOff aria-hidden />} title="โฟลเดอร์นี้ยังไม่มีรูป" description="เมื่อมีรูปเพิ่มในโฟลเดอร์ กด “ตรวจรูปใหม่” เพื่อนำเข้ามาในกิจกรรมนี้" />
          )
        }
      />
    </div>
  );
}

function PersonChip({ active, onClick, label, count, asset, testId }: { active: boolean; onClick: () => void; label: string; count?: number; asset?: string | null; testId: string }) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      data-testid={testId}
      className={cn(
        'inline-flex h-10 max-w-[220px] items-center gap-2 rounded-control border text-sm font-semibold transition-colors',
        asset !== undefined ? 'pr-3.5 pl-1.5' : 'px-4',
        active ? 'border-brand-line bg-brand-soft text-action' : 'border-line-strong bg-surface text-ink hover:bg-sunken',
      )}
    >
      {asset !== undefined && <FaceAvatar asset={asset} name={label} decorative className="size-7 text-xs" />}
      <span className="truncate">{label}</span>
      {count !== undefined && <span className="shrink-0 text-[12.5px] font-normal text-muted">{formatCount(count)}</span>}
    </button>
  );
}

function PeopleTab({ event, onShowPhotos }: { event: EventDetail; onShowPhotos: (personId: string) => void }) {
  const capabilities = useCapabilities();
  if (event.people.length === 0) {
    return (
      <EmptyState
        icon={<UserRoundSearch aria-hidden />}
        title="ยังไม่มีบุคคลที่ยืนยันในงานนี้"
        description={
          capabilities.data?.faceAnalysis.available
            ? 'ตั้งชื่อใบหน้าของงานนี้ได้จากหน้าตรวจสอบใบหน้า หรือคลิกกรอบใบหน้าในตัวดูภาพ รายชื่อจะปรากฏที่นี่'
            : 'เปิดรูปในแท็บรูปภาพ แล้วกด “เพิ่มกรอบใบหน้า” เพื่อบอกว่าเป็นใคร รายชื่อจะปรากฏที่นี่'
        }
      />
    );
  }
  return (
    <div>
      <p className="mb-3 text-sm text-muted">รายชื่อที่ผู้ใช้ยืนยันแล้วในงานนี้ จำนวนรูปนับแบบไม่ซ้ำ (คนหนึ่งมีหลายใบหน้าในรูปเดียวนับครั้งเดียว)</p>
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
        {event.people.map((item) => (
          <li key={item.person.id} className="flex items-center gap-3 rounded-card border border-line bg-surface p-3 shadow-card">
            <FaceAvatar asset={item.person.coverAsset} name={item.person.displayName} shape="rounded" decorative className="size-14 text-xl" />
            <div className="min-w-0 flex-1">
              <Link to={`/people/${item.person.id}`} className="block truncate text-[15.5px] font-bold text-ink hover:text-action hover:underline">
                {item.person.displayName}
              </Link>
              <p className="flex items-center gap-1 text-[13.5px] text-muted">
                <CircleCheck className="size-3.5 text-ok" aria-hidden />
                ยืนยันแล้ว {formatCount(item.photoCount)} รูป
              </p>
            </div>
            <Button size="sm" onClick={() => onShowPhotos(item.person.id)}>
              ดูรูปในงานนี้
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function InfoTab({ event }: { event: EventDetail }) {
  const qc = useQueryClient();
  const [title, setTitle] = useState(event.title);
  const [eventDate, setEventDate] = useState(event.eventDate ?? '');
  const [notes, setNotes] = useState(event.notes);
  const [touched, setTouched] = useState(false);
  const dirty = title !== event.title || eventDate !== (event.eventDate ?? '') || notes !== event.notes;
  const titleError = touched && !title.trim() ? 'กรุณากรอกชื่อกิจกรรม' : null;

  const save = useMutation({
    mutationFn: () => call('update_event', { eventId: event.id, expectedVersion: event.recordVersion, title: title.trim(), eventDate: eventDate || null, notes: notes.trim() }),
    onSuccess: (updated) => {
      qc.setQueryData(qk.event(event.id), updated);
      void qc.invalidateQueries({ queryKey: ['events'] });
      void qc.invalidateQueries({ queryKey: ['photos'] });
      toast.success('บันทึกข้อมูลกิจกรรมแล้ว ชื่อโฟลเดอร์จริงไม่เปลี่ยน');
    },
  });
  const conflict = isErrorCode(save.error, 'VERSION_CONFLICT');

  const copyPath = async () => {
    try {
      await navigator.clipboard.writeText(event.folderPath);
      toast.success('คัดลอกตำแหน่งโฟลเดอร์แล้ว');
    } catch {
      toast.error('คัดลอกไม่สำเร็จ เลือกข้อความแล้วคัดลอกเองได้');
    }
  };

  return (
    <form
      className="max-w-[680px] space-y-4 rounded-card border border-line bg-surface p-6 shadow-card"
      onSubmit={(e) => {
        e.preventDefault();
        setTouched(true);
        if (!title.trim() || !dirty || save.isPending) return;
        save.mutate();
      }}
    >
      <Field label="ชื่อกิจกรรม" error={titleError} hint="แก้เฉพาะชื่อที่แสดงในแอป ไม่เปลี่ยนชื่อโฟลเดอร์จริง">
        {(props) => <TextInput {...props} value={title} invalid={Boolean(titleError)} onChange={(e) => setTitle(e.target.value)} onBlur={() => setTouched(true)} data-testid="event-title-input" />}
      </Field>
      <Field
        label="วันที่กิจกรรม"
        optional
        hint={
          eventDate
            ? `แสดงเป็น ${formatThaiDate(eventDate) ?? eventDate}${event.dateSource === 'folder_name' && eventDate === event.eventDate ? ' · เสนอจากชื่อโฟลเดอร์' : ''} — ไม่เปลี่ยนวันถ่ายในไฟล์รูป`
            : 'ยังไม่ระบุวันที่'
        }
      >
        {(props) => (
          <div className="flex items-center gap-2">
            <TextInput {...props} type="date" value={eventDate} onChange={(e) => setEventDate(e.target.value)} className="max-w-[220px]" />
            {eventDate && (
              <Button variant="ghost" size="sm" onClick={() => setEventDate('')}>
                ล้างวันที่
              </Button>
            )}
          </div>
        )}
      </Field>
      <Field label="หมายเหตุ" optional>
        {(props) => <TextArea {...props} value={notes} onChange={(e) => setNotes(e.target.value)} rows={3} />}
      </Field>
      <div>
        <p className="mb-1.5 text-sm font-semibold text-ink">โฟลเดอร์ต้นทาง</p>
        <div className="flex items-start gap-1.5 rounded-xl bg-sunken px-3 py-2">
          <p className="min-w-0 flex-1 text-[13px] break-all text-muted select-text" data-selectable>
            {event.folderPath}
          </p>
          <IconButton label="คัดลอกตำแหน่งโฟลเดอร์" size="sm" onClick={() => void copyPath()}>
            <Copy className="size-4" aria-hidden />
          </IconButton>
        </div>
      </div>

      {save.isError && (
        <InlineNotice
          tone={conflict ? 'warn' : 'danger'}
          icon={<TriangleAlert aria-hidden />}
          action={
            conflict ? (
              <Button size="sm" onClick={() => void qc.invalidateQueries({ queryKey: qk.event(event.id) })}>
                โหลดข้อมูลล่าสุด
              </Button>
            ) : undefined
          }
        >
          {conflict ? 'ข้อมูลนี้เปลี่ยนหลังจากที่เปิดหน้านี้ กรุณาตรวจข้อมูลล่าสุดก่อนบันทึก' : errorMessage(save.error)}
        </InlineNotice>
      )}

      <div className="flex items-center justify-end gap-2.5 border-t border-line pt-4">
        {dirty && <span className="mr-auto text-[13.5px] text-muted">มีการแก้ไขที่ยังไม่บันทึก</span>}
        <Button
          disabled={!dirty || save.isPending}
          onClick={() => {
            setTitle(event.title);
            setEventDate(event.eventDate ?? '');
            setNotes(event.notes);
            save.reset();
          }}
        >
          คืนค่าเดิม
        </Button>
        <Button type="submit" variant="primary" busy={save.isPending} disabled={!dirty} data-testid="event-save">
          บันทึกการแก้ไข
        </Button>
      </div>
    </form>
  );
}

function HistoryTab({ event }: { event: EventDetail }) {
  if (event.jobs.length === 0) {
    return <EmptyState title="ยังไม่มีประวัติการประมวลผลของกิจกรรมนี้" description="เมื่อเพิ่มหรือตรวจรูปใหม่ ผลของแต่ละรอบจะถูกเก็บไว้ที่นี่" />;
  }
  return (
    <div>
      <p className="mb-3 text-sm text-muted">
        ข้อมูลรูปพร้อม {formatCount(event.thumbReadyCount)} จาก {formatCount(event.photoCount)} รูป
        {event.failedCount > 0 ? ` · อ่านไม่ได้ ${formatCount(event.failedCount)} รูป` : ''}
      </p>
      <ul className="space-y-3">
        {event.jobs.map((job) => (
          <JobCard key={job.id} job={job} />
        ))}
      </ul>
    </div>
  );
}
