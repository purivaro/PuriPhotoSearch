import { useEffect, useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useLocation, useNavigate } from 'react-router';
import { toast } from 'sonner';
import { Calendar, Check, CircleAlert, Folder, FolderPlus, FolderSearch, Image as ImageIcon, Info, Lock, Pencil, SquareArrowOutUpRight, TriangleAlert } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { useStartImport, type ImportLocationState } from '@/app/navigation';
import { Button } from '@/components/ui/Button';
import { EmptyState, ErrorState, InlineNotice, Spinner, StatusBadge } from '@/components/ui/Feedback';
import { Checkbox, Switch, TextInput } from '@/components/ui/Form';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/Overlay';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount, formatThaiDate, isErrorCode, middleEllipsis, newRequestId } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { EventDraft, FaceAnalysisStatus, ImportPreview, SourceMode } from '@/lib/ipc/types';
import { FaceModelNotice } from '@/features/faces/FaceModelNotice';
import { invalidateCatalog, useCapabilities } from '@/lib/queries';
import { buildCommit, draftsForMode, initialEdits, isSelectable, summarize, type DraftEdit, type DraftEdits } from './importModel';

/** S02–S03 — เพิ่มโฟลเดอร์: เลือกแหล่งรูป แล้วตรวจสอบและยืนยัน (สองขั้นเท่านั้น) */
export function ImportReviewPage() {
  useDocumentTitle('เพิ่มโฟลเดอร์กิจกรรม');
  const location = useLocation();
  const state = location.state as ImportLocationState | null;
  if (!state?.path) return <SelectFolderStep />;
  return <ReviewStep key={state.nonce} path={state.path} nonce={state.nonce} />;
}

function Stepper({ step }: { step: 1 | 2 }) {
  const items = [
    { n: 1, label: 'เลือกโฟลเดอร์' },
    { n: 2, label: 'ตรวจสอบรายการ' },
  ] as const;
  return (
    <ol className="mb-4 flex items-center" aria-label="ขั้นตอนการเพิ่มโฟลเดอร์">
      {items.map((item, index) => {
        const done = item.n < step;
        const current = item.n === step;
        return (
          <li key={item.n} className="flex items-center" aria-current={current ? 'step' : undefined}>
            {index > 0 && <span className={cn('mx-2 h-0.5 w-12', done || current ? 'bg-brand' : 'bg-line-strong')} aria-hidden />}
            <span className={cn('flex h-11 items-center gap-3 rounded-full pr-6 pl-1.5 text-[15px] font-semibold', current ? 'bg-brand-soft text-action' : done ? 'bg-brand-soft/60 text-ink' : 'bg-sunken text-muted')}>
              <span className={cn('flex size-8 items-center justify-center rounded-full text-sm font-bold', done || current ? 'bg-brand text-white' : 'bg-line-strong text-surface')}>
                {done ? <Check className="size-4" strokeWidth={3} aria-hidden /> : item.n}
              </span>
              {done ? `${item.n} ${item.label}` : item.label}
              {done && <span className="sr-only">(เสร็จแล้ว)</span>}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

/** ขั้นที่ 1 เมื่อเข้าหน้านี้โดยยังไม่ได้เลือกโฟลเดอร์ */
function SelectFolderStep() {
  const startImport = useStartImport();
  return (
    <Page>
      <PageHeader title="เพิ่มโฟลเดอร์กิจกรรม" subtitle="เลือกโฟลเดอร์งานเดียว หรือโฟลเดอร์ที่รวมหลายงานก็ได้" hideAddFolder />
      <Stepper step={1} />
      <div className="rounded-card border-2 border-dashed border-line-strong bg-surface">
        <EmptyState
          icon={<FolderSearch aria-hidden />}
          title="เลือกโฟลเดอร์ที่เก็บรูปกิจกรรม"
          description="กดปุ่มเพื่อเลือกโฟลเดอร์ หรือลากโฟลเดอร์มาวางบนหน้าต่างนี้ ยังไม่มีอะไรถูกเพิ่มเข้าคลังจนกว่าจะตรวจรายการและกดยืนยัน"
          action={
            <Button variant="primary" size="lg" icon={<FolderPlus className="size-5" aria-hidden />} onClick={() => void startImport()} data-testid="import-pick-folder">
              เลือกโฟลเดอร์
            </Button>
          }
        />
      </div>
    </Page>
  );
}

function ReviewStep({ path, nonce }: { path: string; nonce: string }) {
  const navigate = useNavigate();
  const startImport = useStartImport();
  const qc = useQueryClient();
  const capabilities = useCapabilities();
  // ออกจากหน้านี้โดยไม่ยืนยัน = ยกเลิก preview ไม่เหลือกิจกรรมครึ่ง ๆ กลาง ๆ ในคลัง
  const abandoned = useRef(false);
  const committed = useRef(false);
  const previewIdRef = useRef<string | null>(null);

  const preview = useQuery({
    queryKey: ['import-preview', nonce],
    queryFn: async () => {
      const result = await call('preview_import', { path });
      if (abandoned.current) void call('cancel_import_preview', { previewId: result.previewId }).catch(() => {});
      return result;
    },
    staleTime: Infinity,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });

  useEffect(() => {
    abandoned.current = false;
    return () => {
      abandoned.current = true;
      const id = previewIdRef.current;
      if (id && !committed.current) void call('cancel_import_preview', { previewId: id }).catch(() => {});
    };
  }, []);
  useEffect(() => {
    previewIdRef.current = preview.data?.previewId ?? null;
  }, [preview.data]);

  const leave = () => {
    if (window.history.length > 1) navigate(-1);
    else navigate('/events');
  };

  const pathBar = (
    <div className="mb-4 flex items-center gap-3 rounded-card border border-line bg-surface px-4 py-3 shadow-card">
      <Folder className="size-5 shrink-0 text-muted" aria-hidden />
      <p className="min-w-0 flex-1 truncate text-[15px] font-semibold text-ink" title={path} data-testid="import-root-path">
        {middleEllipsis(path, 90)}
      </p>
      <button type="button" onClick={() => void startImport()} className="inline-flex shrink-0 items-center gap-1.5 text-sm font-semibold text-action hover:underline" data-testid="import-change-folder">
        <SquareArrowOutUpRight className="size-4" aria-hidden />
        เปลี่ยนโฟลเดอร์
      </button>
    </div>
  );

  return (
    <Page>
      <PageHeader title="ตรวจสอบก่อนเพิ่ม" subtitle="เลือกกิจกรรมที่ต้องการ แล้วแก้ชื่อได้ก่อนเริ่ม" hideAddFolder />
      <Stepper step={2} />
      {pathBar}
      {preview.isPending ? (
        <div role="status" aria-live="polite" className="flex flex-col items-center gap-4 rounded-card border border-line bg-surface px-6 py-16 text-center shadow-card" data-testid="import-scanning">
          <Spinner className="size-8" />
          <div>
            <p className="text-lg font-bold text-ink">กำลังตรวจรายการ…</p>
            <p className="mt-1 text-sm text-muted">กำลังอ่านโครงสร้างโฟลเดอร์และนับไฟล์รูป ยังไม่มีอะไรถูกเพิ่มเข้าคลัง</p>
          </div>
          <Button onClick={leave}>ยกเลิก</Button>
        </div>
      ) : preview.isError ? (
        <div className="rounded-card border border-line bg-surface shadow-card">
          <ErrorState
            error={preview.error}
            title="ตรวจรายการในโฟลเดอร์นี้ไม่สำเร็จ"
            onRetry={() => void preview.refetch()}
            extraAction={
              <Button size="sm" onClick={() => void startImport()}>
                เลือกโฟลเดอร์ใหม่
              </Button>
            }
          />
        </div>
      ) : (
        <ReviewForm
          preview={preview.data}
          faceState={capabilities.data?.faceAnalysis.state ?? null}
          faceAnalysisReason={capabilities.data?.faceAnalysis.reason ?? ''}
          folderWatchAvailable={capabilities.data?.folderWatch.available ?? false}
          folderWatchReason={capabilities.data?.folderWatch.reason ?? ''}
          onCancel={leave}
          onRefresh={() => navigate('/import/review', { replace: true, state: { path, nonce: crypto.randomUUID() } satisfies ImportLocationState })}
          onCommitted={(eventIds) => {
            committed.current = true;
            invalidateCatalog(qc);
            const first = eventIds[0];
            if (eventIds.length === 1 && first) navigate(`/events/${first}`, { replace: true });
            else navigate(`/events?new=${eventIds.join(',')}`, { replace: true });
          }}
        />
      )}
    </Page>
  );
}

interface ReviewFormProps {
  preview: ImportPreview;
  /** สถานะตัววิเคราะห์ใบหน้าจาก Rust; null = ยังไม่ทราบ */
  faceState: FaceAnalysisStatus['state'] | null;
  faceAnalysisReason: string;
  folderWatchAvailable: boolean;
  folderWatchReason: string;
  onCancel: () => void;
  onRefresh: () => void;
  onCommitted: (eventIds: string[]) => void;
}

function ReviewForm({
  preview,
  faceState,
  faceAnalysisReason,
  folderWatchAvailable,
  folderWatchReason,
  onCancel,
  onRefresh,
  onCommitted,
}: ReviewFormProps) {
  const [mode, setMode] = useState<SourceMode>(preview.suggestedMode);
  const [edits, setEdits] = useState<DraftEdits>(() => initialEdits(preview));
  const [analyzeFaces, setAnalyzeFaces] = useState(true);
  // เปิดสวิตช์ได้เมื่อพร้อมใช้ หรือกำลังดาวน์โหลดอยู่ (ดาวน์โหลดเสร็จแล้วแอปจะวิเคราะห์รูปในคลังต่อเอง)
  const faceAnalysisAvailable = faceState === 'ready' || faceState === 'installing';
  const [watch, setWatch] = useState(true);
  const [showTitleErrors, setShowTitleErrors] = useState(false);
  // requestId เดียวต่อการยืนยันชุดเดิม: กดซ้ำ/ลองใหม่ไม่สร้างกิจกรรมซ้ำ; แก้รายการแล้วจึงเป็นคำขอใหม่
  const requestId = useRef<string | null>(null);

  const rows = useMemo(() => draftsForMode(preview, mode), [preview, mode]);
  const summary = useMemo(() => summarize(rows, edits), [rows, edits]);
  const selectable = rows.filter(isSelectable);
  const selectedCount = selectable.filter((d) => edits[d.draftId]?.selected).length;
  const hasChildren = preview.children.length > 0;
  const unsupported = Object.entries(preview.unsupportedByExt).sort((a, b) => b[1] - a[1]);
  const unsupportedTotal = unsupported.reduce((sum, [, n]) => sum + n, 0);

  const patch = (draftId: string, change: Partial<DraftEdit>) => {
    requestId.current = null;
    setEdits((current) => ({ ...current, [draftId]: { ...(current[draftId] as DraftEdit), ...change } }));
  };
  const setAll = (selected: boolean) => {
    requestId.current = null;
    setEdits((current) => {
      const next = { ...current };
      for (const draft of selectable) next[draft.draftId] = { ...(next[draft.draftId] as DraftEdit), selected };
      return next;
    });
  };
  const changeMode = (next: SourceMode) => {
    requestId.current = null;
    setMode(next);
  };

  const commit = useMutation({
    mutationFn: () => {
      requestId.current ??= newRequestId();
      return call('commit_import', buildCommit(preview, mode, edits, { requestId: requestId.current, analyzeFaces: analyzeFaces && faceAnalysisAvailable, watch: watch && folderWatchAvailable }));
    },
    onSuccess: (result) => {
      const parts = [];
      if (result.createdEvents > 0) parts.push(`เพิ่ม ${formatCount(result.createdEvents)} กิจกรรม`);
      if (result.reusedEvents > 0) parts.push(`ตรวจรูปใหม่ใน ${formatCount(result.reusedEvents)} กิจกรรมเดิม`);
      toast.success(
        faceAnalysisAvailable && analyzeFaces
          ? 'เพิ่มกิจกรรมแล้ว กำลังวิเคราะห์ใบหน้า ใช้รูปที่พร้อมได้ทันที'
          : `${parts.join(' · ') || 'เพิ่มกิจกรรมแล้ว'} กำลังเตรียมข้อมูลรูป ใช้รูปที่พร้อมได้ทันที`,
      );
      onCommitted(result.eventIds);
    },
  });

  const submit = () => {
    if (summary.missingTitles > 0) {
      setShowTitleErrors(true);
      return;
    }
    if (summary.eventCount === 0 || commit.isPending) return;
    commit.mutate();
  };

  const stale = isErrorCode(commit.error, 'PREVIEW_STALE');
  const confirmLabel =
    summary.eventCount === 0
      ? 'เลือกกิจกรรมที่จะเพิ่ม'
      : summary.newPhotos === 0
        ? `เชื่อม ${formatCount(summary.eventCount)} โฟลเดอร์ไว้รอรูป`
        : `เพิ่ม ${formatCount(summary.eventCount)} กิจกรรม · ${formatCount(summary.newPhotos)} รูป`;

  return (
    <div data-testid="import-review">
      <fieldset className="mb-3 flex flex-wrap items-center gap-x-6 gap-y-2">
        <legend className="sr-only">วิธีแบ่งกิจกรรมจากโฟลเดอร์นี้</legend>
        <ModeRadio value="single_event" current={mode} onChange={changeMode} label="งานเดียว" testId="import-mode-single" />
        <ModeRadio value="child_events" current={mode} onChange={changeMode} label="หลายงาน" disabled={!hasChildren} testId="import-mode-children" />
        <p className="border-l border-line-strong pl-4 text-sm text-muted">
          {mode === 'single_event'
            ? 'กิจกรรมเดียว — รวมรูปในโฟลเดอร์ย่อย'
            : 'แยกโฟลเดอร์ย่อยเป็นหลายกิจกรรม — รูปในชั้นลึกกว่านั้นอยู่ในกิจกรรมของโฟลเดอร์ชั้นแรก'}
          {!hasChildren && ' · โฟลเดอร์นี้ไม่มีโฟลเดอร์ย่อย'}
        </p>
      </fieldset>

      {mode === 'single_event' && hasChildren && preview.suggestedMode === 'child_events' && (
        <InlineNotice tone="info" icon={<Info aria-hidden />} className="mb-3">
          พบ {formatCount(preview.children.length)} โฟลเดอร์ย่อยในโฟลเดอร์นี้ ถ้าแต่ละโฟลเดอร์เป็นคนละงาน เลือก “หลายงาน” เพื่อแยกเป็นหลายกิจกรรม
        </InlineNotice>
      )}

      <div className="overflow-hidden rounded-card border border-line bg-surface shadow-card">
        <div className="grid grid-cols-[44px_132px_minmax(180px,1.5fr)_minmax(140px,1fr)_170px_128px] items-center gap-x-4 border-b border-line bg-canvas px-4 py-2.5 text-[13.5px] font-bold text-ink max-[1100px]:grid-cols-[44px_104px_minmax(160px,1fr)_150px_112px]">
          <Checkbox
            label="เลือกทุกกิจกรรม"
            testId="import-select-all"
            checked={selectable.length > 0 && selectedCount === selectable.length ? true : selectedCount > 0 ? 'indeterminate' : false}
            onCheckedChange={(checked) => setAll(checked)}
            disabled={selectable.length === 0}
          />
          <span>ตัวอย่างภาพ</span>
          <span>
            ชื่อกิจกรรม <span className="font-normal text-muted">(แก้ไขได้)</span>
          </span>
          <span className="max-[1100px]:hidden">โฟลเดอร์จริง</span>
          <span>วันที่กิจกรรม</span>
          <span className="text-right">จำนวนรูป</span>
        </div>
        {rows.length === 0 ? (
          <EmptyState compact icon={<Folder aria-hidden />} title="ไม่พบโฟลเดอร์ย่อยหรือรูปในโฟลเดอร์นี้" description="ลองเลือกโหมด “งานเดียว” หรือเลือกโฟลเดอร์อื่น" />
        ) : (
          <ul>
            {rows.map((draft) => (
              <DraftRow
                key={draft.draftId}
                previewId={preview.previewId}
                draft={draft}
                edit={edits[draft.draftId] as DraftEdit}
                showTitleError={showTitleErrors}
                onChange={(change) => patch(draft.draftId, change)}
              />
            ))}
          </ul>
        )}
      </div>

      {unsupportedTotal > 0 && (
        <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />} className="mt-3">
          ข้ามไฟล์ภาพ {formatCount(unsupportedTotal)} ไฟล์ที่รุ่นนี้ยังเปิดไม่ได้:{' '}
          {unsupported.map(([ext, n]) => `.${ext} ${formatCount(n)} ไฟล์`).join(' · ')} — ไฟล์เหล่านี้ยังอยู่ในโฟลเดอร์เดิมและไม่ถูกแก้ไข
        </InlineNotice>
      )}

      <div className="mt-4 flex flex-wrap items-start gap-x-8 gap-y-3">
        <Switch
          checked={analyzeFaces && faceAnalysisAvailable}
          onCheckedChange={setAnalyzeFaces}
          disabled={!faceAnalysisAvailable}
          testId="import-analyze-faces"
          label="วิเคราะห์ใบหน้าเมื่อเพิ่มเสร็จ"
          description={
            faceState === 'ready'
              ? 'ทำเบื้องหลัง ใช้รูปที่พร้อมได้ระหว่างรอ'
              : faceState === 'installing'
                ? 'จะเริ่มเองเมื่อดาวน์โหลดตัววิเคราะห์ใบหน้าเสร็จ'
                : faceState === 'not_installed'
                  ? 'ยังไม่ได้ดาวน์โหลดตัววิเคราะห์ใบหน้า — กดดาวน์โหลดด้านล่างได้เลย เพิ่มกิจกรรมได้โดยไม่ต้องรอ'
                  : faceAnalysisReason || 'เครื่องนี้ยังวิเคราะห์ใบหน้าอัตโนมัติไม่ได้ ระบุใบหน้าเองได้จากตัวดูภาพ'
          }
        />
        <Switch
          checked={watch && folderWatchAvailable}
          onCheckedChange={setWatch}
          disabled={!folderWatchAvailable}
          testId="import-watch"
          label="ตรวจหารูปใหม่ขณะเปิดแอป"
          description={folderWatchAvailable ? 'เฉพาะรูปที่เพิ่มในกิจกรรมเดิม และตรวจซ้ำเมื่อเปิดแอปครั้งต่อไป' : folderWatchReason || 'รุ่นนี้ยังไม่ติดตามรูปใหม่อัตโนมัติ'}
        />
        <p className="flex items-center gap-1.5 pt-0.5 text-sm text-muted">
          <Info className="size-4 shrink-0" aria-hidden />
          โฟลเดอร์งานใหม่ภายหลัง ต้องเลือกเพิ่มกิจกรรมเอง
        </p>
      </div>
      {(faceState === 'not_installed' || faceState === 'installing') && <FaceModelNotice variant="bar" className="mt-3" />}

      {commit.isError && (
        <InlineNotice
          tone={stale ? 'warn' : 'danger'}
          icon={<CircleAlert aria-hidden />}
          className="mt-4"
          action={
            stale ? (
              <Button size="sm" onClick={onRefresh} data-testid="import-refresh-preview">
                ตรวจรายการใหม่
              </Button>
            ) : undefined
          }
        >
          {stale ? 'โฟลเดอร์เปลี่ยนไปหลังจากตรวจรายการนี้ หรือรายการหมดอายุแล้ว กรุณาตรวจรายการใหม่ก่อนเพิ่ม' : errorMessage(commit.error)}
        </InlineNotice>
      )}

      <div className="sticky bottom-3 z-10 mt-4 flex flex-wrap items-center gap-x-5 gap-y-3 rounded-card border border-brand-line bar-brand px-5 py-3.5 shadow-raised" data-testid="import-summary">
        <div className="min-w-0 flex-1">
          <p className="text-lg font-bold text-ink" aria-live="polite">
            เลือก {formatCount(summary.eventCount)} กิจกรรม · {formatCount(summary.newPhotos)} รูปที่จะเพิ่มใหม่
          </p>
          <p className="text-[13.5px] text-muted">
            {[
              summary.existingPhotos > 0 ? `มีอยู่ในคลังแล้ว ${formatCount(summary.existingPhotos)} รูป (ไม่เพิ่มซ้ำ)` : null,
              summary.unselectedPhotos > 0 ? `ยังไม่ได้เลือก ${formatCount(summary.unselectedPhotos)} รูป` : null,
              'ไม่ย้ายหรือเปลี่ยนชื่อรูปต้นฉบับ',
            ]
              .filter(Boolean)
              .join(' · ')}
          </p>
          {showTitleErrors && summary.missingTitles > 0 && (
            <p role="alert" className="text-[13.5px] font-semibold text-danger">
              มี {formatCount(summary.missingTitles)} กิจกรรมที่ยังไม่มีชื่อ กรุณาตั้งชื่อก่อนเพิ่ม
            </p>
          )}
        </div>
        <Button size="lg" onClick={onCancel} disabled={commit.isPending} data-testid="import-cancel">
          ย้อนกลับ
        </Button>
        <Button size="lg" variant="primary" busy={commit.isPending} disabled={summary.eventCount === 0} onClick={submit} data-testid="import-commit">
          {confirmLabel}
        </Button>
      </div>
    </div>
  );
}

function ModeRadio({ value, current, onChange, label, disabled, testId }: { value: SourceMode; current: SourceMode; onChange: (mode: SourceMode) => void; label: string; disabled?: boolean; testId: string }) {
  const checked = value === current;
  return (
    <label className={cn('flex items-center gap-2.5 text-[15px] font-semibold', disabled ? 'cursor-not-allowed text-faint' : 'cursor-pointer text-ink')}>
      <input type="radio" name="import-mode" className="peer sr-only" checked={checked} disabled={disabled} onChange={() => onChange(value)} data-testid={testId} />
      <span
        aria-hidden
        className={cn(
          'flex size-[22px] items-center justify-center rounded-full border-2 peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-brand',
          checked ? 'border-brand' : 'border-line-strong',
        )}
      >
        {checked && <span className="size-3 rounded-full bg-brand" />}
      </span>
      {label}
    </label>
  );
}

function DraftRow({ previewId, draft, edit, showTitleError, onChange }: { previewId: string; draft: EventDraft; edit: DraftEdit; showTitleError: boolean; onChange: (change: Partial<DraftEdit>) => void }) {
  const selectable = isSelectable(draft);
  const [sampleFailed, setSampleFailed] = useState(false);
  const existing = Boolean(draft.existingEventId);
  const titleMissing = showTitleError && edit.selected && !edit.title.trim();
  const dateText = formatThaiDate(edit.eventDate);
  const dateFromFolder = draft.dateSource === 'folder_name' && edit.eventDate === draft.suggestedDate && draft.suggestedDate !== null;

  return (
    <li
      data-testid="import-draft-row"
      data-draft-id={draft.draftId}
      data-draft-kind={draft.kind}
      className={cn(
        'grid grid-cols-[44px_132px_minmax(180px,1.5fr)_minmax(140px,1fr)_170px_128px] items-center gap-x-4 border-b border-line px-4 py-3 last:border-b-0 max-[1100px]:grid-cols-[44px_104px_minmax(160px,1fr)_150px_112px]',
        !selectable && 'bg-canvas/60',
      )}
    >
      <Checkbox
        label={`เลือก ${edit.title || draft.folderName}`}
        testId="import-draft-select"
        checked={edit.selected && selectable}
        disabled={!selectable}
        onCheckedChange={(selected) => onChange({ selected })}
        className="data-[state=checked]:border-action"
      />
      <div className="aspect-[3/2] w-full overflow-hidden rounded-lg bg-sunken">
        {draft.hasSample && !sampleFailed ? (
          <img src={assetUrl('draft', `${previewId}/${draft.draftId}`)} alt="" loading="lazy" onError={() => setSampleFailed(true)} className="size-full object-cover" />
        ) : (
          <div className="flex size-full items-center justify-center text-faint">{draft.readable ? <ImageIcon className="size-6" aria-hidden /> : <Lock className="size-6" aria-hidden />}</div>
        )}
      </div>
      <div className="min-w-0">
        <div className="relative">
          <TextInput
            data-testid="import-draft-title"
            aria-label={`ชื่อกิจกรรมของโฟลเดอร์ ${draft.folderName}`}
            value={edit.title}
            invalid={titleMissing}
            disabled={!selectable || existing}
            onChange={(event) => onChange({ title: event.target.value })}
            placeholder="ชื่อกิจกรรม"
            className="h-9 border-transparent bg-transparent pr-8 pl-2 text-[15.5px] font-bold hover:border-line-strong focus:border-brand focus:bg-surface disabled:border-transparent disabled:bg-transparent disabled:text-ink"
          />
          {selectable && !existing && <Pencil className="pointer-events-none absolute top-1/2 right-2.5 size-3.5 -translate-y-1/2 text-brand" aria-hidden />}
        </div>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1 pl-2 text-[13px] text-muted">
          {draft.kind === 'root_loose' ? <span>รูปที่อยู่ในโฟลเดอร์นี้โดยตรง</span> : <span className="truncate min-[1101px]:hidden">{draft.folderName}</span>}
          {existing && (
            <>
              <StatusBadge tone="info">อยู่ในคลังแล้ว</StatusBadge>
              <Link to={`/events/${draft.existingEventId}`} className="font-semibold text-action hover:underline">
                เปิดกิจกรรมเดิม
              </Link>
            </>
          )}
          {!draft.readable && (
            <StatusBadge tone="danger" icon={<Lock aria-hidden />}>
              {draft.unreadableReason ?? 'อ่านโฟลเดอร์นี้ไม่ได้'}
            </StatusBadge>
          )}
          {titleMissing && <span className="font-semibold text-danger">กรุณาตั้งชื่อกิจกรรม</span>}
        </div>
      </div>
      <p className="min-w-0 truncate text-sm text-muted max-[1100px]:hidden" title={draft.displayPath}>
        {draft.folderName}
      </p>
      <div className="min-w-0">
        <Popover>
          <PopoverTrigger asChild>
            <button
              type="button"
              disabled={!selectable || existing}
              data-testid="import-draft-date"
              className="flex max-w-full items-center gap-2 rounded-lg px-1.5 py-1 text-left text-[15px] font-semibold text-ink hover:bg-sunken disabled:hover:bg-transparent"
            >
              <Calendar className="size-4 shrink-0 text-muted" aria-hidden />
              <span className={cn('truncate', !dateText && 'font-normal text-muted')}>{dateText ?? 'ยังไม่ระบุวันที่'}</span>
            </button>
          </PopoverTrigger>
          <PopoverContent className="w-[260px] p-3.5">
            <label className="block text-sm font-semibold text-ink">
              วันที่กิจกรรม
              <TextInput type="date" className="mt-1.5" value={edit.eventDate ?? ''} onChange={(event) => onChange({ eventDate: event.target.value || null })} />
            </label>
            <p className="mt-2 text-[12.5px] text-muted">แก้เฉพาะข้อมูลในแอป ไม่เปลี่ยนชื่อโฟลเดอร์หรือวันถ่ายในไฟล์รูป</p>
            <div className="mt-2.5 flex justify-between gap-2">
              <Button size="sm" variant="ghost" onClick={() => onChange({ eventDate: null })} disabled={!edit.eventDate}>
                ไม่ระบุวันที่
              </Button>
              {draft.suggestedDate && edit.eventDate !== draft.suggestedDate && (
                <Button size="sm" onClick={() => onChange({ eventDate: draft.suggestedDate })}>
                  ใช้ค่าที่เสนอ
                </Button>
              )}
            </div>
          </PopoverContent>
        </Popover>
        <p className="pl-1.5 text-[12.5px] text-muted">{dateFromFolder ? 'เสนอจากชื่อโฟลเดอร์' : edit.eventDate ? 'กำหนดเอง' : draft.kind === 'root_loose' ? 'ไม่มีวันที่ในชื่อ' : 'ชื่อโฟลเดอร์ไม่ระบุวันที่ชัดเจน'}</p>
      </div>
      <div className="text-right">
        <p className="flex items-center justify-end gap-1.5 text-[15.5px] font-semibold text-ink">
          <ImageIcon className="size-4 text-muted" aria-hidden />
          {formatCount(draft.photoCount)} รูป
        </p>
        <p className="text-[12.5px] text-muted">
          {!draft.readable
            ? 'ตรวจไม่ได้'
            : draft.photoCount === 0
              ? 'ไม่มีไฟล์รูปที่รองรับ'
              : draft.existingCount > 0
                ? `ใหม่ ${formatCount(draft.newCount)} · มีแล้ว ${formatCount(draft.existingCount)}`
                : edit.selected
                  ? 'จะเพิ่มใหม่ทั้งหมด'
                  : 'ยังไม่เลือก'}
          {draft.unsupportedCount > 0 ? ` · ไม่รองรับ ${formatCount(draft.unsupportedCount)}` : ''}
        </p>
      </div>
    </li>
  );
}
