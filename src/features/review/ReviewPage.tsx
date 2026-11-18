import { useEffect, useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router';
import { toast } from 'sonner';
import { Calendar, CircleCheck, EllipsisVertical, Info, RefreshCw, Sparkles, UserRoundPen, UserRoundSearch, X } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { useViewer } from '@/app/navigation';
import { Button, IconButton } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { EmptyState, ErrorState, InlineNotice, LoadingBlock } from '@/components/ui/Feedback';
import { Checkbox, Select, Tabs } from '@/components/ui/Form';
import { Menu, MenuContent, MenuItem, MenuTrigger, Popover, PopoverContent, PopoverTrigger } from '@/components/ui/Overlay';
import { FaceModelNotice } from '@/features/faces/FaceModelNotice';
import { LevelBadge } from '@/features/faces/LevelBadge';
import { FaceAvatar } from '@/features/people/FaceAvatar';
import { PersonPicker, type PersonChoice } from '@/features/people/PersonPicker';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount, isErrorCode, newRequestId } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { Id, PersonSummary, ReviewFace, ReviewOverview } from '@/lib/ipc/types';
import { invalidatePeopleData, isJobActive, qk, useActiveJobs, useCapabilities, useEvents } from '@/lib/queries';
import { defaultTab, photoIdsOf, queueGroups, resolveGroup, selectAllFaces, selectedFaceIds, selectionCheckState, toggleFace, type QueueGroup, type ReviewTab } from './reviewModel';

/**
 * S16 — ตรวจสอบใบหน้า
 * ตั้งชื่อและแก้ข้อเสนอของ AI เป็นกลุ่ม โดยผู้ใช้เห็นครบว่ากำลังยืนยันใบหน้าใดบ้าง
 * ข้อเสนอของ AI ไม่ใช่คำยืนยัน: จะกลายเป็นชื่อจริงก็ต่อเมื่อผู้ใช้กดยืนยันใบหน้าที่เลือกเท่านั้น
 */
export function ReviewPage() {
  useDocumentTitle('ตรวจสอบใบหน้า');
  const [params, setParams] = useSearchParams();
  const eventId = params.get('event');
  const tabParam = params.get('tab');
  const capabilities = useCapabilities();
  const events = useEvents('');
  // คิวเป็นภาพ ณ ตอนที่โหลด: ไม่โหลดใหม่เองเมื่อสลับหน้าต่างหรือระหว่างงานวิเคราะห์ เพื่อไม่ให้รายการที่กำลังตรวจสลับตำแหน่งโดยไม่แจ้ง
  // โหลดใหม่เมื่อผู้ใช้ทำคำสั่งเอง (ยืนยัน/ปฏิเสธ) หรือกด "อัปเดตคิว"
  const overview = useQuery({
    queryKey: qk.reviewOverview(eventId),
    queryFn: () => call('get_review_overview', eventId ? { eventId } : {}),
    staleTime: Infinity,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
  const jobs = useActiveJobs();
  const analysing = (jobs.data ?? []).some((job) => job.kind === 'catalog' && isJobActive(job));
  // งานวิเคราะห์เพิ่งจบหลังจากโหลดคิวนี้ → มีรายการใหม่ที่ยังไม่แสดง
  const [outdated, setOutdated] = useState(false);
  const wasAnalysing = useRef(false);
  useEffect(() => {
    if (analysing) wasAnalysing.current = true;
    else if (wasAnalysing.current) {
      wasAnalysing.current = false;
      setOutdated(true);
    }
  }, [analysing]);
  useEffect(() => setOutdated(false), [overview.dataUpdatedAt]);

  const patchParams = (patch: Record<string, string | null>) => {
    const next = new URLSearchParams(params);
    for (const [key, value] of Object.entries(patch)) {
      if (value) next.set(key, value);
      else next.delete(key);
    }
    setParams(next, { replace: true });
  };

  const face = capabilities.data?.faceAnalysis;
  const tab: ReviewTab = tabParam === 'unknown' || tabParam === 'suggested' ? tabParam : overview.data ? defaultTab(overview.data) : 'suggested';
  const scopeEvent = eventId ? events.data?.find((e) => e.id === eventId) : undefined;

  return (
    <Page>
      <PageHeader title="ตรวจสอบใบหน้า" subtitle="ตรวจชื่อให้ถูก แล้วค้นรูปครั้งต่อไปได้ง่ายขึ้น" />

      <div className="mb-4 flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm font-semibold text-ink">
          <Calendar className="size-[18px] text-muted" aria-hidden />
          <span className="sr-only">ขอบเขตกิจกรรม</span>
          <Select value={eventId ?? ''} onChange={(e) => patchParams({ event: e.target.value || null })} className="w-[300px] max-w-full" data-testid="review-event-scope">
            <option value="">ทุกกิจกรรม</option>
            {eventId && !scopeEvent && <option value={eventId}>กิจกรรมที่เลือก</option>}
            {(events.data ?? []).map((event) => (
              <option key={event.id} value={event.id}>
                {event.title}
              </option>
            ))}
          </Select>
        </label>
        {overview.data && (
          <p className="text-sm text-muted" data-testid="review-unnamed-count">
            {overview.data.unnamedFaceCount > 0 ? `มี ${formatCount(overview.data.unnamedFaceCount)} ใบหน้าที่ยังไม่มีชื่อในขอบเขตนี้` : 'ไม่มีใบหน้าที่รอตั้งชื่อในขอบเขตนี้'}
          </p>
        )}
      </div>

      {face && face.state !== 'ready' && <FaceModelNotice className="mb-5 max-w-[860px]" />}

      {overview.isPending ? (
        <LoadingBlock label="กำลังจัดกลุ่มใบหน้าที่รอตรวจ…" />
      ) : overview.isError ? (
        <ErrorState error={overview.error} title="โหลดคิวตรวจสอบใบหน้าไม่สำเร็จ" onRetry={() => void overview.refetch()} />
      ) : (
        <>
          {(analysing || outdated) && (
            <InlineNotice
              tone="info"
              icon={<Info aria-hidden />}
              className="mb-4 max-w-[860px]"
              action={
                <Button size="sm" icon={<RefreshCw className="size-3.5" aria-hidden />} busy={overview.isFetching} onClick={() => void overview.refetch()} data-testid="review-refresh">
                  อัปเดตคิว
                </Button>
              }
            >
              {analysing
                ? 'แอปกำลังวิเคราะห์ใบหน้าอยู่ คิวนี้แสดงรายการ ณ ตอนที่เปิด — กด “อัปเดตคิว” เมื่อต้องการดูรายการล่าสุด'
                : 'วิเคราะห์ใบหน้าเสร็จเพิ่มแล้ว อาจมีรายการใหม่ที่ยังไม่แสดง — กด “อัปเดตคิว”'}
            </InlineNotice>
          )}
          {!analysing && overview.data.photosPendingAnalysis > 0 && <PendingAnalysisNotice count={overview.data.photosPendingAnalysis} eventId={eventId} canAnalyze={Boolean(face?.available)} />}
          <ReviewWorkspace overview={overview.data} eventId={eventId} tab={tab} onTabChange={(next) => patchParams({ tab: next })} modelReady={face?.state === 'ready'} />
        </>
      )}
    </Page>
  );
}

function PendingAnalysisNotice({ count, eventId, canAnalyze }: { count: number; eventId: Id | null; canAnalyze: boolean }) {
  const qc = useQueryClient();
  const analyze = useMutation({
    mutationFn: () => call('analyze_faces', eventId ? { eventId } : {}),
    onSuccess: (result) => {
      void qc.invalidateQueries({ queryKey: ['jobs'] });
      toast.success(result.jobIds.length > 0 ? 'เริ่มวิเคราะห์ใบหน้าเบื้องหลังแล้ว รายการใหม่จะเข้าคิวนี้เมื่อเสร็จ' : 'ไม่มีรูปที่ต้องวิเคราะห์เพิ่มในขอบเขตนี้');
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
  return (
    <InlineNotice
      tone="info"
      icon={<Info aria-hidden />}
      className="mb-4 max-w-[860px]"
      action={
        canAnalyze ? (
          <Button size="sm" busy={analyze.isPending} onClick={() => analyze.mutate()} data-testid="review-analyze">
            วิเคราะห์ตอนนี้
          </Button>
        ) : undefined
      }
    >
      ยังมี {formatCount(count)} รูปในขอบเขตนี้ที่ยังไม่ได้วิเคราะห์ใบหน้า — คิวนี้อาจมีรายการเพิ่มภายหลัง
    </InlineNotice>
  );
}

function ReviewWorkspace({ overview, eventId, tab, onTabChange, modelReady }: { overview: ReviewOverview; eventId: Id | null; tab: ReviewTab; onTabChange: (tab: ReviewTab) => void; modelReady: boolean }) {
  const groups = useMemo(() => queueGroups(overview, tab), [overview, tab]);
  // กลุ่มที่ผู้ใช้เลือก จำทั้ง ID และตำแหน่ง: เมื่อกลุ่มนั้นถูกจัดการจนหมด กลุ่มถัดไปจะเลื่อนขึ้นมาแทน
  const [picked, setPicked] = useState<{ tab: ReviewTab; id: Id; index: number } | null>(null);
  const staleRetries = useRef(0);
  const selectedId = resolveGroup(groups, picked && picked.tab === tab ? picked : null);
  const selectedIndex = groups.findIndex((g) => g.id === selectedId);
  const selected = selectedIndex >= 0 ? groups[selectedIndex] : undefined;

  useEffect(() => {
    if (selectedId && (!picked || picked.tab !== tab || picked.id !== selectedId || picked.index !== selectedIndex)) setPicked({ tab, id: selectedId, index: selectedIndex });
  }, [selectedId, selectedIndex, picked, tab]);

  const nothingAtAll = overview.suggested.length === 0 && overview.clusters.length === 0;
  if (nothingAtAll) {
    return <AllDone overview={overview} eventId={eventId} modelReady={modelReady} />;
  }

  const suggestedPerson = tab === 'suggested' && selected ? overview.suggested.find((g) => g.person.id === selected.id)?.person : undefined;

  return (
    <div data-testid="review-workspace">
      <Tabs<ReviewTab>
        label="ชนิดรายการที่รอตรวจ"
        testId="review-tab"
        value={tab}
        onChange={onTabChange}
        className="mb-4"
        items={[
          { value: 'suggested', label: 'AI เสนอชื่อ', badge: <CountBadge count={overview.suggested.length} /> },
          { value: 'unknown', label: 'ยังไม่ทราบชื่อ', badge: <CountBadge count={overview.clusters.length} /> },
        ]}
      />

      {groups.length === 0 ? (
        <div className="rounded-card border border-dashed border-line-strong bg-surface">
          <EmptyState
            icon={tab === 'suggested' ? <Sparkles aria-hidden /> : <UserRoundSearch aria-hidden />}
            title={tab === 'suggested' ? 'ไม่มีข้อเสนอชื่อจาก AI ในขอบเขตนี้' : 'ไม่มีกลุ่มใบหน้าที่ยังไม่ทราบชื่อในขอบเขตนี้'}
            description={
              tab === 'suggested'
                ? 'AI เสนอชื่อได้เมื่อมีบุคคลที่ยืนยันใบหน้าไว้แล้ว เริ่มจากตั้งชื่อกลุ่มในแท็บ “ยังไม่ทราบชื่อ”'
                : 'ใบหน้าที่เหลือถูกเสนอชื่อไว้ในแท็บ “AI เสนอชื่อ” แล้ว'
            }
            action={
              <Button onClick={() => onTabChange(tab === 'suggested' ? 'unknown' : 'suggested')} data-testid="review-switch-tab">
                {tab === 'suggested' ? 'เปิดแท็บยังไม่ทราบชื่อ' : 'เปิดแท็บ AI เสนอชื่อ'}
              </Button>
            }
          />
        </div>
      ) : (
        <div className={cn('grid items-start gap-4', suggestedPerson ? 'grid-cols-[minmax(240px,300px)_minmax(0,1fr)] xl:grid-cols-[300px_minmax(0,1fr)_280px]' : 'grid-cols-[minmax(240px,300px)_minmax(0,1fr)]')}>
          <QueueList
            groups={groups}
            tab={tab}
            overview={overview}
            selectedId={selectedId}
            onSelect={(group, index) => setPicked({ tab, id: group.id, index })}
          />
          {selected && (
            <GroupPanel
              key={`${tab}:${selected.id}`}
              group={selected}
              groupNumber={selectedIndex + 1}
              person={suggestedPerson}
              eventId={eventId}
              reviewRevision={overview.reviewRevision}
              staleRetries={staleRetries}
            />
          )}
          {suggestedPerson && <SuggestedPersonPanel person={suggestedPerson} className="col-span-2 xl:col-span-1" />}
        </div>
      )}

      {tab === 'unknown' && overview.singleFaceCount > 0 && (
        <p className="mt-4 flex items-start gap-2 text-[13.5px] text-muted" data-testid="review-single-faces">
          <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
          <span>
            อีก {formatCount(overview.singleFaceCount)} ใบหน้ายังไม่เข้ากลุ่มใด (พบครั้งเดียว ไม่คล้ายใบหน้าอื่นพอ หรือภาพไม่ชัดพอให้ AI เสนอชื่อ) ตั้งชื่อได้จากตัวดูภาพโดยคลิกที่กรอบใบหน้า —{' '}
            <Link to={`/search?face=no_person${eventId ? `&e=${eventId}` : ''}`} className="font-semibold text-action hover:underline">
              ดูรูปที่ยังไม่มีชื่อบุคคล
            </Link>
          </span>
        </p>
      )}
    </div>
  );
}

function CountBadge({ count }: { count: number }) {
  if (count === 0) return null;
  return <span className="rounded-full bg-sunken px-1.5 text-[12px] font-bold text-muted">{formatCount(count)}</span>;
}

function AllDone({ overview, eventId, modelReady }: { overview: ReviewOverview; eventId: Id | null; modelReady: boolean }) {
  const pending = overview.photosPendingAnalysis > 0;
  return (
    <div className="rounded-card border border-dashed border-line-strong bg-surface" data-testid="review-empty">
      <EmptyState
        icon={<CircleCheck aria-hidden />}
        title="ตรวจรายการในขอบเขตนี้ครบแล้ว"
        description={
          <>
            {pending
              ? 'ยังมีรูปที่ยังไม่ได้วิเคราะห์ใบหน้า จึงอาจมีรายการใหม่เข้ามาภายหลัง'
              : modelReady
                ? 'ไม่มีกลุ่มใบหน้าที่รอตั้งชื่อหรือรอยืนยันในตอนนี้'
                : 'ยังไม่มีใบหน้าที่ระบบตรวจพบให้ตรวจ — ดาวน์โหลดตัววิเคราะห์ใบหน้าด้านบน หรือระบุใบหน้าเองจากตัวดูภาพได้'}
            {overview.singleFaceCount > 0 && <span className="mt-1.5 block">มีอีก {formatCount(overview.singleFaceCount)} ใบหน้าเดี่ยวที่ยังไม่มีชื่อ ตั้งชื่อได้จากตัวดูภาพโดยคลิกที่กรอบใบหน้า</span>}
          </>
        }
        action={
          <>
            <Link to="/events" className="inline-flex h-10 items-center rounded-control border border-line-strong bg-surface px-4 text-sm font-semibold text-ink shadow-card hover:bg-sunken" data-testid="review-open-events">
              เปิดกิจกรรม
            </Link>
            {overview.singleFaceCount > 0 && (
              <Link
                to={`/search?face=no_person${eventId ? `&e=${eventId}` : ''}`}
                className="inline-flex h-10 items-center rounded-control bg-action px-4 text-sm font-semibold text-on-action hover:bg-action-hover"
              >
                ดูรูปที่ยังไม่มีชื่อบุคคล
              </Link>
            )}
          </>
        }
      />
    </div>
  );
}

function QueueList({
  groups,
  tab,
  overview,
  selectedId,
  onSelect,
}: {
  groups: QueueGroup[];
  tab: ReviewTab;
  overview: ReviewOverview;
  selectedId: Id | null;
  onSelect: (group: QueueGroup, index: number) => void;
}) {
  return (
    <section aria-labelledby="review-queue" className="rounded-card border border-line bg-surface p-3 shadow-card">
      <h2 id="review-queue" className="px-1.5 pt-1 pb-2.5 text-[17px] font-bold text-ink">
        คิวตรวจสอบใบหน้า
      </h2>
      <ul className="scroll-thin -mr-1.5 max-h-[calc(100vh-330px)] min-h-[240px] space-y-2 overflow-y-auto pr-1.5" data-testid="review-queue">
        {groups.map((group, index) => {
          const active = group.id === selectedId;
          const person = tab === 'suggested' ? overview.suggested.find((g) => g.person.id === group.id)?.person : undefined;
          const title = person ? `อาจเป็น ${person.displayName}` : `กลุ่มที่ ${String(index + 1).padStart(2, '0')}`;
          return (
            <li key={group.id}>
              <button
                type="button"
                aria-current={active ? 'true' : undefined}
                onClick={() => onSelect(group, index)}
                data-testid="review-group"
                data-group-id={group.id}
                data-group-kind={group.kind}
                className={cn(
                  'block w-full rounded-xl border px-3 py-2.5 text-left transition-colors',
                  active ? 'border-brand bg-brand-soft ring-2 ring-brand/40' : 'border-line bg-surface hover:bg-sunken',
                )}
              >
                <span className="flex items-baseline justify-between gap-2">
                  <span className="min-w-0 truncate text-[15px] font-bold text-ink">{title}</span>
                  <span className="shrink-0 text-[13px] text-muted">{formatCount(group.faceCount)} ใบหน้า</span>
                </span>
                <span className="mt-2 flex gap-1.5 overflow-hidden" aria-hidden>
                  {group.sampleFaceIds.slice(0, 6).map((faceId) => (
                    <img key={faceId} src={assetUrl('face', faceId)} alt="" loading="lazy" className="size-10 shrink-0 rounded-lg bg-sunken object-cover" />
                  ))}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function SuggestedPersonPanel({ person, className }: { person: PersonSummary; className?: string }) {
  return (
    <aside aria-labelledby="review-suggested-person" className={cn('rounded-card border border-line bg-surface p-5 shadow-card', className)} data-testid="review-suggested-person">
      <h2 id="review-suggested-person" className="text-[17px] font-bold text-ink">
        AI เสนอว่าเป็น
      </h2>
      <div className="mt-3 flex items-center gap-3.5">
        <FaceAvatar asset={person.coverAsset} name={person.displayName} shape="rounded" className="size-[88px] rounded-2xl text-3xl" />
        <div className="min-w-0">
          <p className="truncate text-xl font-bold text-ink">{person.displayName}</p>
          {person.nickname && <p className="truncate text-sm text-muted">{person.nickname}</p>}
          <p className="mt-0.5 text-[13.5px] text-muted">ยืนยันแล้ว {formatCount(person.confirmedPhotoCount)} รูป</p>
        </div>
      </div>
      <p className="mt-3 text-[13px] leading-relaxed text-muted">
        เทียบใบหน้าในกลุ่มกับภาพตัวแทนนี้ก่อนยืนยัน ข้อเสนอของ AI ยังไม่ถูกนับเป็นรูปของบุคคลนี้จนกว่าจะกด “ใช่”
      </p>
      <Link to={`/people/${person.id}`} className="mt-3 inline-flex h-9 items-center rounded-lg border border-line-strong bg-surface px-3.5 text-sm font-semibold text-ink shadow-card hover:bg-sunken">
        ดูรูปที่ยืนยันแล้วของบุคคลนี้
      </Link>
    </aside>
  );
}

interface AssignArgs {
  faceIds: Id[];
  target: { personId: Id } | { newPersonName: string };
  name: string;
}

function GroupPanel({
  group,
  groupNumber,
  person,
  eventId,
  reviewRevision,
  staleRetries,
}: {
  group: QueueGroup;
  groupNumber: number;
  person: PersonSummary | undefined;
  eventId: Id | null;
  reviewRevision: number;
  /** ตัวนับการโหลดคิวใหม่เพราะ PREVIEW_STALE — อยู่ระดับหน้า จึงไม่ถูกล้างเมื่อกลุ่มที่เลือกเปลี่ยนตามคิวใหม่ */
  staleRetries: React.RefObject<number>;
}) {
  const qc = useQueryClient();
  const viewer = useViewer();
  const facesQuery = useQuery({
    queryKey: qk.reviewFaces(group.kind, group.id, eventId, reviewRevision),
    queryFn: () => call('get_review_faces', { kind: group.kind, id: group.id, reviewRevision, ...(eventId ? { eventId } : {}) }),
    retry: false,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
  const stale = facesQuery.isError && isErrorCode(facesQuery.error, 'PREVIEW_STALE');
  // คิวเปลี่ยนไปแล้ว (มีการตัดสินจากหน้าอื่น เช่นตัวดูภาพ): โหลดคิวใหม่เงียบ ๆ ได้ไม่เกินสองครั้ง
  // ถ้ายังไม่ทัน ให้หยุดและบอกผู้ใช้ ไม่วนถามซ้ำไม่สิ้นสุด (ใบหน้าใหม่จากงานวิเคราะห์ไม่ทำให้คิวที่เปิดอยู่เก่าแล้ว)
  const gaveUp = stale && staleRetries.current >= 2;
  useEffect(() => {
    if (facesQuery.isSuccess) staleRetries.current = 0;
    if (stale && staleRetries.current < 2) {
      staleRetries.current += 1;
      void qc.invalidateQueries({ queryKey: ['review', 'overview'] });
    }
  }, [stale, facesQuery.isSuccess, facesQuery.errorUpdatedAt, qc]);

  const faces = useMemo(() => facesQuery.data?.faces ?? [], [facesQuery.data]);
  // null = ค่าเริ่มต้น "เลือกทุกใบที่โหลด"; หลังทำคำสั่งแล้วกลุ่มยังเหลือใบหน้า จะเริ่มจากไม่เลือก เพื่อไม่ให้กดซ้ำแล้วยืนยันใบที่เหลือโดยไม่ตั้งใจ
  const [selection, setSelection] = useState<Set<Id> | null>(null);
  const effective = selection ?? selectAllFaces(faces);
  const chosen = selectedFaceIds(faces, effective);
  const count = chosen.length;
  const [picking, setPicking] = useState(false);
  const [confirmIgnore, setConfirmIgnore] = useState(false);

  const done = () => {
    setSelection(new Set());
    invalidatePeopleData(qc);
  };
  const onError = (error: unknown) => toast.error(errorMessage(error));
  const assign = useMutation({
    mutationFn: (args: AssignArgs) => call('assign_faces', { requestId: newRequestId(), faceIds: args.faceIds, ...args.target }),
    onSuccess: (_result, args) => {
      toast.success(`ยืนยัน ${formatCount(args.faceIds.length)} ใบหน้าเป็น ${args.name} แล้ว ค้นรูปเหล่านี้จากชื่อได้ทันที`);
      done();
    },
    onError,
  });
  const reject = useMutation({
    mutationFn: (args: { faceIds: Id[]; person: PersonSummary }) => call('reject_suggestions', { faceIds: args.faceIds, personId: args.person.id }),
    onSuccess: (_result, args) => {
      toast.success(`จะไม่เสนอ ${args.person.displayName} ให้ ${formatCount(args.faceIds.length)} ใบหน้านี้อีก ใบหน้ายังอยู่ให้ตั้งชื่อเป็นคนอื่นได้`);
      done();
    },
    onError,
  });
  const ignore = useMutation({
    mutationFn: (faceIds: Id[]) => call('ignore_faces', { faceIds }),
    onSuccess: (_result, faceIds) => {
      toast.success(`เอา ${formatCount(faceIds.length)} กรอบออกแล้ว (ไม่ใช่ใบหน้า) รูปต้นฉบับไม่ถูกแก้ไข`);
      done();
    },
    onError,
  });
  const busy = assign.isPending || reject.isPending || ignore.isPending;

  const pick = (choice: PersonChoice) => {
    if (chosen.length === 0) {
      // รายการถูกโหลดใหม่ระหว่างเปิดช่องตั้งชื่อ: ไม่ส่งคำสั่งที่ไม่มีใบหน้า
      setPicking(false);
      toast.error('รายการใบหน้าเปลี่ยนไปแล้ว กรุณาเลือกใบหน้าอีกครั้ง');
      return;
    }
    const args: AssignArgs =
      choice.kind === 'existing'
        ? { faceIds: chosen, target: { personId: choice.person.id }, name: choice.person.displayName }
        : { faceIds: chosen, target: { newPersonName: choice.name }, name: choice.name };
    assign.mutate(args, { onSuccess: () => setPicking(false) });
  };

  const openPhoto = (face: ReviewFace) => {
    const ids = photoIdsOf(faces);
    viewer.open(face.photoId, { ids, total: ids.length, originLabel: 'กลับไปตรวจสอบใบหน้า', owner: 'review' });
  };

  const heading = person ? `ใบหน้าที่ AI เสนอว่าเป็น ${person.displayName}` : `กลุ่มที่ ${String(groupNumber).padStart(2, '0')} — ยังไม่ทราบชื่อ`;

  return (
    <section aria-label={heading} className="min-w-0 rounded-card border border-line bg-surface shadow-card" data-testid="review-group-panel">
      {gaveUp ? (
        <EmptyState
          compact
          icon={<RefreshCw aria-hidden />}
          title="คิวกำลังเปลี่ยนระหว่างที่โหลดกลุ่มนี้"
          description="มีการยืนยันหรือแก้ชื่อใบหน้าจากหน้าอื่นระหว่างที่โหลดกลุ่มนี้ กดโหลดอีกครั้งเพื่อดูรายการล่าสุด"
          action={
            <Button
              size="sm"
              icon={<RefreshCw className="size-3.5" aria-hidden />}
              onClick={() => {
                staleRetries.current = 0;
                void qc.invalidateQueries({ queryKey: ['review', 'overview'] });
              }}
              data-testid="review-reload-group"
            >
              โหลดอีกครั้ง
            </Button>
          }
        />
      ) : facesQuery.isPending || stale ? (
        <LoadingBlock label="กำลังโหลดใบหน้าในกลุ่ม…" />
      ) : facesQuery.isError ? (
        <ErrorState error={facesQuery.error} title="โหลดใบหน้าในกลุ่มนี้ไม่สำเร็จ" onRetry={() => void facesQuery.refetch()} />
      ) : faces.length === 0 ? (
        <EmptyState compact icon={<CircleCheck aria-hidden />} title="กลุ่มนี้ไม่มีใบหน้ารอตรวจแล้ว" description="เลือกกลุ่มอื่นจากคิวด้านซ้าย" />
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-line px-4 py-3">
            <Checkbox
              checked={selectionCheckState(faces, effective)}
              onCheckedChange={(checked) => setSelection(checked ? selectAllFaces(faces) : new Set())}
              label={count === faces.length ? 'ไม่เลือกใบหน้าใดเลย' : 'เลือกใบหน้าทั้งหมดที่แสดง'}
              testId="review-select-all"
            />
            <h2 className="text-[17px] font-bold text-ink" aria-live="polite" data-testid="review-selection-count">
              เลือก {formatCount(count)} จาก {formatCount(faces.length)} ใบหน้า
            </h2>
            {facesQuery.data.total > faces.length && (
              <p className="text-[13px] text-muted" data-testid="review-truncated">
                แสดง {formatCount(faces.length)} ใบแรกจากทั้งหมด {formatCount(facesQuery.data.total)} ใบหน้า — ที่เหลือจะแสดงหลังตรวจชุดนี้
              </p>
            )}
          </div>

          <ul className="grid grid-cols-[repeat(auto-fill,minmax(128px,1fr))] gap-3 p-4" data-testid="review-face-grid">
            {faces.map((face) => {
              const checked = effective.has(face.faceId);
              return (
                <li key={face.faceId} className="relative min-w-0" data-testid="review-face" data-face-id={face.faceId}>
                  <button
                    type="button"
                    onClick={() => openPhoto(face)}
                    aria-label={`เปิดรูป ${face.filename} จาก ${face.eventTitle} เพื่อดูบริบท`}
                    title={`${face.filename} · ${face.eventTitle}`}
                    data-testid="review-face-open"
                    className={cn('block w-full overflow-hidden rounded-xl border-[2.5px] bg-sunken transition-shadow hover:shadow-raised', checked ? 'border-brand' : 'border-line opacity-70')}
                  >
                    <img src={assetUrl('face', face.faceId)} alt="" loading="lazy" decoding="async" className="aspect-square w-full object-cover" />
                  </button>
                  <Checkbox
                    checked={checked}
                    onCheckedChange={() => setSelection(toggleFace(effective, face.faceId))}
                    label={`เลือกใบหน้าจากรูป ${face.filename}`}
                    testId="review-face-select"
                    className="absolute top-2 left-2 shadow-raised"
                  />
                  {face.level && (
                    <span className="mt-1 block">
                      <LevelBadge level={face.level} />
                    </span>
                  )}
                  <span className="mt-0.5 block truncate text-[12.5px] text-muted">{face.eventTitle}</span>
                </li>
              );
            })}
          </ul>

          <div className="bar-brand sticky bottom-3 z-10 mx-3 mb-3 flex flex-wrap items-center gap-x-3 gap-y-2.5 rounded-xl border border-brand-line px-4 py-3 shadow-raised" data-testid="review-actions">
            <p className="flex min-w-[200px] flex-1 items-center gap-2 text-[15px] font-bold text-ink">
              <Info className="size-[18px] shrink-0 text-action" aria-hidden />
              {count > 0 ? `การยืนยันจะใช้กับ ${formatCount(count)} ใบหน้าที่เลือกเท่านั้น` : 'ยังไม่ได้เลือกใบหน้า ติ๊กใบหน้าที่ต้องการก่อน'}
            </p>

            {person ? (
              <>
                <Button
                  variant="primary"
                  icon={<CircleCheck className="size-[18px]" aria-hidden />}
                  disabled={count === 0 || busy}
                  busy={assign.isPending && !picking}
                  onClick={() => assign.mutate({ faceIds: chosen, target: { personId: person.id }, name: person.displayName })}
                  data-testid="review-confirm"
                >
                  ใช่ {person.displayName} · ยืนยัน {formatCount(count)} ใบหน้า
                </Button>
                <Button variant="danger" icon={<X className="size-4" aria-hidden />} disabled={count === 0 || busy} busy={reject.isPending} onClick={() => reject.mutate({ faceIds: chosen, person })} data-testid="review-reject">
                  ไม่ใช่ {person.displayName}
                </Button>
              </>
            ) : null}

            <Popover open={picking} onOpenChange={setPicking}>
              <PopoverTrigger asChild>
                <Button variant={person ? 'secondary' : 'primary'} icon={<UserRoundPen className="size-[18px]" aria-hidden />} disabled={count === 0 || busy} data-testid="review-name">
                  {person ? 'เป็นคนอื่น…' : `ตั้งชื่อ ${formatCount(count)} ใบหน้า…`}
                </Button>
              </PopoverTrigger>
              <PopoverContent align="end" side="top" aria-label={`ตั้งชื่อ ${formatCount(count)} ใบหน้าที่เลือก`}>
                <p className="border-b border-line px-3 pt-2.5 pb-2 text-sm font-bold text-ink">
                  {formatCount(count)} ใบหน้าที่เลือกคือใคร
                </p>
                <PersonPicker busy={assign.isPending} excludeIds={person ? [person.id] : []} onPick={pick} />
              </PopoverContent>
            </Popover>

            <Menu>
              <MenuTrigger asChild>
                <IconButton label="คำสั่งเพิ่มเติมของใบหน้าที่เลือก" disabled={count === 0 || busy} data-testid="review-more">
                  <EllipsisVertical className="size-5" aria-hidden />
                </IconButton>
              </MenuTrigger>
              <MenuContent>
                <MenuItem tone="danger" icon={<X />} onSelect={() => setConfirmIgnore(true)}>
                  ไม่ใช่ใบหน้า ({formatCount(count)})
                </MenuItem>
              </MenuContent>
            </Menu>
          </div>
        </>
      )}

      <ConfirmDialog
        open={confirmIgnore}
        onOpenChange={setConfirmIgnore}
        title={`เอา ${formatCount(count)} กรอบออกเพราะไม่ใช่ใบหน้า?`}
        description="กรอบที่เลือกจะไม่แสดงและไม่ถูกเสนอชื่ออีก รูปต้นฉบับและบุคคลไม่ถูกลบ"
        confirmLabel="เอากรอบออก"
        tone="danger"
        busy={ignore.isPending}
        onConfirm={() => ignore.mutate(chosen, { onSettled: () => setConfirmIgnore(false) })}
      />
    </section>
  );
}
