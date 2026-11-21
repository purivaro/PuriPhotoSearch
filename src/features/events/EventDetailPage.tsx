import { useMemo, useRef, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Link, useParams, useSearchParams } from 'react-router';
import { toast } from 'sonner';
import { CircleCheck, CloudOff, Copy, FolderOpen, FolderSearch, Image as ImageIcon, ImageOff, Pin, RefreshCw, ScanFace, SlidersHorizontal, Sparkles, TriangleAlert, UserRoundSearch } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { Button, IconButton } from '@/components/ui/Button';
import { EmptyState, ErrorState, InlineNotice, LoadingBlock, StatusBadge } from '@/components/ui/Feedback';
import { Field, Select, Tabs, TextArea, TextInput } from '@/components/ui/Form';
import { JobCard, JobStrip } from '@/features/jobs/JobCard';
import { FaceAvatar } from '@/features/people/FaceAvatar';
import { ThumbSizeSlider } from '@/features/photos/ThumbSizeSlider';
import { EditFilterSelect, EditProgress, progressPhrase } from '@/features/photos/EditProgress';
import { FlagFilterSelect, FlagSummary } from '@/features/photos/PhotoFlags';
import { CullDialog } from '@/features/photos/CullDialog';
import { PhotoResults } from '@/features/photos/PhotoResults';
import { parseEditFilter, parseFlagFilter } from '@/features/search/searchParams';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount, formatThaiDate, isErrorCode, middleEllipsis, newRequestId } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { EditFilter, EventDetail, FlagFilter, SearchParams } from '@/lib/ipc/types';
import { isJobActive, PHOTO_PAGE_SIZE, qk, useActiveJobs, useCapabilities, useEvent, type PhotoQuery } from '@/lib/queries';
import { FILE_MANAGER } from '@/lib/platform';
import { useFolderChooser } from '@/app/FolderChooser';
import { RelinkDialog } from './RelinkDialog';

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
  const setParam = (key: string, value: string | null) => patchParams({ [key]: value });
  // เปลี่ยนหลายค่าใน URL ครั้งเดียว (เรียก setParam ติดกันหลายครั้งจะทับกัน เพราะทุกครั้งเริ่มจาก params ชุดเดิม)
  const patchParams = (patch: Record<string, string | null>) => {
    const next = new URLSearchParams(params);
    for (const [key, value] of Object.entries(patch)) {
      if (value === null) next.delete(key);
      else next.set(key, value);
    }
    setParams(next, { replace: true });
  };
  // แต่งตามคน (DECISIONS ข้อ 32): เลือกคนในงาน → เปิดรูปแรกของเขาที่ยังไม่แต่งในหน้าแต่งภาพ (ค่า = คนที่จะแต่ง)
  const [autoEdit, setAutoEdit] = useState<string | null>(null);

  const reveal = useMutation({
    mutationFn: () => call('reveal_event_folder', { eventId }),
    onError: (error) => toast.error(errorMessage(error)),
  });
  // หนึ่ง requestId ต่อการกดหนึ่งครั้ง: กดซ้ำระหว่างรอไม่สร้างงานสำเนา
  const choose = useFolderChooser();
  const relinkPreview = useMutation({
    mutationFn: (newFolder: string) => call('preview_relink', { eventId, newFolder }),
    onError: (error) => toast.error(errorMessage(error)),
  });
  const chooseNewFolder = async () => {
    const folder = await choose({ title: 'เลือกโฟลเดอร์ของกิจกรรมนี้ที่ตำแหน่งใหม่', hint: 'โฟลเดอร์ที่มีรูปของกิจกรรมนี้อยู่ตอนนี้ แอปจะตรวจให้ดูก่อนเปลี่ยนตำแหน่ง' });
    if (folder) relinkPreview.mutate(folder);
  };
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
            เปิดใน {FILE_MANAGER}
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
        <InlineNotice
          tone="warn"
          icon={<CloudOff aria-hidden />}
          className="mb-4"
          action={
            <Button size="sm" icon={<FolderSearch className="size-4" aria-hidden />} busy={relinkPreview.isPending} onClick={() => void chooseNewFolder()} data-testid="event-relink">
              ระบุตำแหน่งใหม่…
            </Button>
          }
        >
          หาโฟลเดอร์ต้นทางไม่เจอ ถ้าอยู่ในไดรฟ์ภายนอก ให้เชื่อมต่อไดรฟ์แล้วเปิดกิจกรรมนี้อีกครั้ง ถ้าย้ายหรือเปลี่ยนชื่อโฟลเดอร์ไปแล้ว ให้ระบุตำแหน่งใหม่
          ระหว่างนี้ยังดูภาพย่อ ค้นหา และดูชื่อบุคคลได้ตามเดิม แต่เปิดต้นฉบับ ส่งออก และตรวจรูปใหม่ไม่ได้
        </InlineNotice>
      )}
      <RelinkDialog preview={relinkPreview.data ?? null} onClose={() => relinkPreview.reset()} />

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

      {tab === 'photos' && (
        <PhotosTab
          event={data}
          personId={params.get('person')}
          onPersonChange={(id) => setParam('person', id)}
          editFilter={parseEditFilter(params.get('ed'))}
          flagFilter={parseFlagFilter(params.get('fl'))}
          onFiltersChange={({ person, ed, fl }) =>
            patchParams({
              ...(person !== undefined && { person }),
              ...(ed !== undefined && { ed: ed === 'any' ? null : ed }),
              ...(fl !== undefined && { fl: fl === 'any' ? null : fl }),
            })
          }
          scanning={Boolean(job)}
          // เริ่มเมื่อ URL เปลี่ยนเป็นคนนั้น + "ยังไม่แต่ง" แล้วจริง (react-router เปลี่ยน URL แบบ transition ช้ากว่า state นี้)
          autoEdit={autoEdit !== null && autoEdit === params.get('person') && params.get('ed') === 'unedited'}
          onAutoEditHandled={(opened) => {
            setAutoEdit(null);
            if (!opened) toast.success('รูปของคนนี้ในงานนี้แต่งครบแล้ว');
          }}
        />
      )}
      {tab === 'people' && (
        <PeopleTab
          event={data}
          onShowPhotos={(id) => setParams(new URLSearchParams({ person: id }), { replace: true })}
          onEditPhotos={(id) => {
            setParams(new URLSearchParams({ person: id, ed: 'unedited' }), { replace: true });
            setAutoEdit(id);
          }}
        />
      )}
      {tab === 'info' && <InfoTab key={data.recordVersion} event={data} />}
      {tab === 'history' && <HistoryTab event={data} />}
    </Page>
  );
}

function PhotosTab({
  event,
  personId,
  onPersonChange,
  editFilter,
  flagFilter,
  onFiltersChange,
  scanning,
  autoEdit,
  onAutoEditHandled,
}: {
  event: EventDetail;
  personId: string | null;
  onPersonChange: (id: string | null) => void;
  editFilter: EditFilter;
  flagFilter: FlagFilter;
  onFiltersChange: (patch: { person?: string | null; ed?: EditFilter; fl?: FlagFilter }) => void;
  scanning: boolean;
  autoEdit: boolean;
  onAutoEditHandled: (opened: boolean) => void;
}) {
  const [faceFilter, setFaceFilter] = useState<SearchParams['faceFilter']>('any');
  const [sort, setSort] = useState<SearchParams['sort']>('taken_asc');
  const [cullOpen, setCullOpen] = useState(false);
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
      // คนในงานนับรูปที่ AI จัดให้ด้วย (ตรงกับตัวเลขบนชิป) · ป้ายชื่อบนรูปบอกคนที่ AI จัดให้ด้วยรูปประกาย
      includeSuggested: true,
      editFilter,
      flagFilter,
    }),
    [event.id, personId, faceFilter, sort, editFilter, flagFilter],
  );
  const filtered = Boolean(personId) || faceFilter !== 'any' || editFilter !== 'any' || flagFilter !== 'any';

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
              pinned={item.pinned}
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
        <EditFilterSelect value={editFilter} onChange={(ed) => onFiltersChange({ ed })} />
        <FlagFilterSelect value={flagFilter} onChange={(fl) => onFiltersChange({ fl })} />
        <Button icon={<Sparkles className="size-4" aria-hidden />} onClick={() => setCullOpen(true)} data-testid="event-cull-open">
          AI ช่วยคัด
        </Button>
        <CullDialog eventId={event.id} open={cullOpen} onOpenChange={setCullOpen} onShowPicked={() => onFiltersChange({ fl: 'picked' })} />
        <Select value={sort} onChange={(e) => setSort(e.target.value as SearchParams['sort'])} aria-label="เรียงรูปตาม">
          <option value="taken_asc">วันถ่าย เก่า → ใหม่</option>
          <option value="taken_desc">วันถ่าย ใหม่ → เก่า</option>
          <option value="filename">ชื่อไฟล์</option>
          <option value="added_desc">เพิ่มล่าสุด</option>
        </Select>
        <ThumbSizeSlider />
      </div>
      <PhotoResults
        sizeControl={false}
        query={query}
        label={`รูปในกิจกรรม ${event.title}`}
        originLabel="กลับไปกิจกรรม"
        selectionKey={`${event.id}|${personId ?? ''}|${faceFilter}|${editFilter}|${flagFilter}`}
        autoEdit={autoEdit}
        onAutoEditHandled={onAutoEditHandled}
        summary={(meta) =>
          meta.total > 0 || (meta.coverage?.eligiblePhotos ?? 0) > 0 ? (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
              {filtered && (
                <p className="text-sm text-muted" aria-live="polite">
                  พบ {formatCount(meta.total)} รูป{progressPhrase(editFilter, flagFilter)}จาก {formatCount(event.photoCount)} รูปในงานนี้
                  {selectedPerson ? ` ที่มี ${selectedPerson.displayName} (รวมที่ AI จัดให้)` : ''}
                </p>
              )}
              <EditProgress meta={meta} />
              <FlagSummary coverage={meta.coverage} />
            </div>
          ) : null
        }
        empty={
          editFilter === 'unedited' && faceFilter === 'any' ? (
            <EmptyState
              icon={<CircleCheck aria-hidden />}
              title={`แต่งรูป${flagFilter === 'any' ? '' : progressPhrase('any', flagFilter)}${selectedPerson ? `ของ ${selectedPerson.displayName} ` : ''}ในงานนี้ครบแล้ว`}
              description={`ไม่มีรูป${progressPhrase(editFilter, flagFilter)} — รูปที่แต่งแล้วมีป้าย “แต่งแล้ว” กำกับ`}
              action={<Button onClick={() => onFiltersChange({ ed: 'any', fl: 'any' })}>ดูทุกรูป</Button>}
            />
          ) : filtered ? (
            <EmptyState
              icon={<UserRoundSearch aria-hidden />}
              title="ไม่มีรูปที่ตรงกับตัวกรองนี้ในงานนี้"
              description="ลองล้างตัวกรองเพื่อดูรูปทั้งหมดของกิจกรรม"
              action={
                <Button
                  onClick={() => {
                    setFaceFilter('any');
                    onFiltersChange({ person: null, ed: 'any', fl: 'any' });
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

function PersonChip({
  active,
  onClick,
  label,
  count,
  asset,
  pinned,
  testId,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  count?: number;
  asset?: string | null;
  pinned?: boolean;
  testId: string;
}) {
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
      {pinned && <Pin className="size-3.5 shrink-0 text-action" fill="currentColor" aria-label="ปักหมุดไว้" />}
      <span className="truncate">{label}</span>
      {count !== undefined && <span className="shrink-0 text-[12.5px] font-normal text-muted">{formatCount(count)}</span>}
    </button>
  );
}

function PeopleTab({ event, onShowPhotos, onEditPhotos }: { event: EventDetail; onShowPhotos: (personId: string) => void; onEditPhotos: (personId: string) => void }) {
  const capabilities = useCapabilities();
  if (event.people.length === 0) {
    return (
      <EmptyState
        icon={<UserRoundSearch aria-hidden />}
        title="ยังไม่พบบุคคลในงานนี้"
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
      <p className="mb-3 text-sm text-muted">
        คนในงานนี้ ทั้งที่ยืนยันแล้วและที่ AI จัดให้ (ยังไม่มีคนตรวจ) · คนที่ปักหมุดไว้ขึ้นก่อน · จำนวนรูปนับแบบไม่ซ้ำ · กด “แต่งรูป” เพื่อแต่งรูปของคนนั้นที่ยังไม่แต่งทีละรูป
      </p>
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-3">
        {event.people.map((item) => {
          const confirmed = item.photoCount - item.aiPhotoCount;
          return (
            <li key={item.person.id} className="flex items-center gap-3 rounded-card border border-line bg-surface p-3 shadow-card" data-testid="event-person-row">
              <FaceAvatar asset={item.person.coverAsset} name={item.person.displayName} shape="rounded" decorative className="size-14 text-xl" />
              <div className="min-w-0 flex-1">
                <Link to={`/people/${item.person.id}`} className="flex items-center gap-1.5 text-[15.5px] font-bold text-ink hover:text-action hover:underline">
                  {item.pinned && <Pin className="size-3.5 shrink-0 text-action" fill="currentColor" aria-label="ปักหมุดไว้" />}
                  <span className="truncate">{item.person.displayName}</span>
                </Link>
                <p className="flex flex-wrap items-center gap-x-2 text-[13px] text-muted">
                  <span className="font-semibold text-ink">{formatCount(item.photoCount)} รูป</span>
                  {confirmed > 0 && (
                    <span className="inline-flex items-center gap-1">
                      <CircleCheck className="size-3.5 text-ok" aria-hidden />
                      ยืนยัน {formatCount(confirmed)}
                    </span>
                  )}
                  {item.aiPhotoCount > 0 && (
                    <span className="inline-flex items-center gap-1">
                      <Sparkles className="size-3.5 text-brand" aria-hidden />
                      AI จัดให้ {formatCount(item.aiPhotoCount)}
                    </span>
                  )}
                </p>
                <p className="text-[12.5px] text-muted" data-testid="event-person-unedited">
                  {item.uneditedPhotoCount > 0 ? `ยังไม่แต่ง ${formatCount(item.uneditedPhotoCount)} รูป` : 'แต่งครบทุกรูปแล้ว'}
                </p>
              </div>
              <div className="flex shrink-0 flex-col gap-1.5">
                <Button
                  size="sm"
                  variant="outline"
                  icon={<SlidersHorizontal className="size-4" aria-hidden />}
                  disabled={item.uneditedPhotoCount === 0}
                  onClick={() => onEditPhotos(item.person.id)}
                  data-testid="event-person-edit"
                >
                  แต่งรูป
                </Button>
                <Button size="sm" onClick={() => onShowPhotos(item.person.id)}>
                  ดูรูปในงานนี้
                </Button>
              </div>
            </li>
          );
        })}
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
