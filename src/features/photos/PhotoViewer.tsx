import { useCallback, useEffect, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useLocation, useNavigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { ArrowLeft, Ban, ChevronLeft, ChevronRight, ClipboardPaste, Copy, EllipsisVertical, ExternalLink, Flag, FolderOpen, Image as ImageIcon, Minus, PanelRight, Plus, ScanFace, Palette, SlidersHorizontal, Sparkles, Undo2, Upload, X } from 'lucide-react';
import { useViewer, viewerPhotoIdNow, type ViewerLocationState } from '@/app/navigation';
import { Button, IconButton } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { Menu, MenuContent, MenuItem, MenuTrigger } from '@/components/ui/Overlay';
import { ExportDialog } from '@/features/export/ExportDialog';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { Id, PhotoDetails, PhotoFlag } from '@/lib/ipc/types';
import { qk } from '@/lib/queries';
import { useEditClipboard } from '@/stores/editClipboard';
import { PasteEditDialog } from './PasteEditDialog';
import { FLAG_LABEL, flagForKey, useSetPhotoFlags } from './PhotoFlags';
import { GenerateDialog } from '@/features/assistant/GenerateDialog';
import { useAssistant } from '@/stores/assistant';
import { useUi } from '@/stores/ui';
import { PhotoEditor, type EditorTab } from './PhotoEditor';
import { ViewerInspector } from './ViewerInspector';
import { ViewerStage, type ViewerStageHandle } from './ViewerStage';
import { FILE_MANAGER } from '@/lib/platform';

const FILMSTRIP_RADIUS = 7;

function isTypingTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  return el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable;
}

function overlayOpen(): boolean {
  return Boolean(document.querySelector('[role="dialog"][data-state="open"], [role="menu"][data-state="open"], [data-radix-popper-content-wrapper]'));
}

/**
 * S15 — ตัวดูภาพ วางทับพื้นที่เนื้อหาโดยหน้าที่มายังอยู่ข้างหลัง
 * ปิดแล้วจึงกลับไปที่ตำแหน่งเลื่อน ตัวกรอง และรูปที่เลือกไว้เดิม
 */
export function PhotoViewer() {
  const { photoId = '' } = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const viewer = useViewer();
  const qc = useQueryClient();
  const state = location.state as ViewerLocationState | null;
  const stage = useRef<ViewerStageHandle>(null);

  const [showFaces, setShowFaces] = useState(true);
  const [drawMode, setDrawMode] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [selectedFaceId, setSelectedFaceId] = useState<Id | null>(null);
  const [zoom, setZoom] = useState(1);
  const [exporting, setExporting] = useState(false);
  const [editing, setEditing] = useState(false);
  const [confirmRevert, setConfirmRevert] = useState(false);
  const [pasting, setPasting] = useState(false);
  const [generating, setGenerating] = useState(false);
  const clipboard = useEditClipboard();

  const details = useQuery({ queryKey: qk.photo(photoId), queryFn: () => call('get_photo_details', { photoId }) });
  const data = details.data?.id === photoId ? details.data : undefined;

  const session = viewer.session.current;
  const ids = session.ids;
  const index = ids.indexOf(photoId);
  const total = Math.max(session.total ?? ids.length, ids.length);
  const previousId = index > 0 ? ids[index - 1] : undefined;
  const nextId = index >= 0 && index < ids.length - 1 ? ids[index + 1] : undefined;

  // แต่งทีละรูป (DECISIONS ข้อ 32): เปิดมาที่หน้าแต่งภาพเลย หรือเปิดหน้าแต่งภาพต่อให้รูปถัดไปหลัง "บันทึก แล้วไปรูปถัดไป" / "ข้าม"
  // `editFor` = รูปที่ต้องเปิดหน้าแต่งภาพให้ · รันซ้ำได้ผลเดิม (StrictMode รัน effect สองรอบ) และล้างเมื่อไปรูปอื่นด้วยวิธีอื่น
  const editFor = useRef<Id | null>(null);
  // แท็บของหน้าแต่งภาพที่ใช้ล่าสุดในตัวดูภาพนี้ (รูปถัดไปเปิดที่แท็บเดิม)
  const editorTab = useRef<EditorTab>('crop');
  const sessionRef = viewer.session;
  useEffect(() => {
    setDrawMode(false);
    setSelectedFaceId(null);
    const opened = sessionRef.current;
    if (opened.edit) {
      editFor.current = photoId;
      sessionRef.current = { ...opened, edit: false };
    } else if (editFor.current !== photoId) {
      editFor.current = null;
    }
    setEditing(editFor.current === photoId);
  }, [photoId, sessionRef]);

  // บันทึกการแต่งแล้ว: ใช้ข้อมูลใหม่ทันที และให้ภาพย่อในหน้าที่มาเปลี่ยนตาม
  const applyEdited = useCallback(
    (result: PhotoDetails) => {
      qc.setQueryData(qk.photo(result.id), result);
      void qc.invalidateQueries({ queryKey: ['photos'] });
      setEditing(false);
    },
    [qc],
  );
  const revert = useMutation({
    mutationFn: () => call('reset_photo_edit', { photoId }),
    onSuccess: (result) => {
      applyEdited(result);
      setConfirmRevert(false);
      toast.success('กลับเป็นภาพต้นฉบับแล้ว');
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  // ใกล้ท้ายของรูปที่โหลดแล้ว → ขอให้หน้าที่มาโหลดหน้าถัดไป เพื่อกดถัดไปต่อได้จนครบชุดผล
  const loadedCount = ids.length;
  useEffect(() => {
    if (index >= 0 && index >= loadedCount - 4) viewer.session.current.requestMore?.();
  }, [index, loadedCount, viewer.session, viewer.version]);

  const goTo = useCallback(
    (id: Id | undefined) => {
      // เปลี่ยนรูปแทนที่ history เดิม: ปิดครั้งเดียวกลับถึงหน้าที่มา
      if (id) navigate(`/photos/${id}`, { replace: true, state: location.state });
    },
    [navigate, location.state],
  );

  // ไปรูปถัดไปโดยเปิดหน้าแต่งภาพค้างไว้ · โหลดข้อมูลของรูปถัดไปไว้ก่อน หน้าแต่งภาพจึงเปิดต่อได้ทันที
  const editNext = nextId
    ? (result?: PhotoDetails) => {
        if (result) {
          qc.setQueryData(qk.photo(result.id), result);
          void qc.invalidateQueries({ queryKey: ['photos'] });
        }
        editFor.current = nextId;
        goTo(nextId);
      }
    : null;
  useQuery({
    queryKey: qk.photo(nextId ?? ''),
    queryFn: () => call('get_photo_details', { photoId: nextId as Id }),
    enabled: editing && Boolean(nextId),
  });

  // คัดก่อนแต่ง (DECISIONS ข้อ 33): P เลือก · X ไม่เอา แล้วไปรูปถัดไปเอง · U ล้าง — เห็นผลทันที บันทึกตามหลัง
  const setFlags = useSetPhotoFlags();
  // คัดรูปที่อยู่ใน URL ตอนกดจริง: กด P แล้ว X ติดกันเร็ว ๆ — URL เปลี่ยนเป็นรูปถัดไปแล้วแต่ React อาจยังวาดรูปเดิม
  // ถ้าใช้ state ของรอบที่วาดอยู่ X จะไปทับรูปก่อนหน้า (e2e จับได้) · คัดได้แม้รายละเอียดของรูปยังโหลดไม่เสร็จ
  const mutateFlags = useRef(setFlags.mutate);
  mutateFlags.current = setFlags.mutate;
  const sessionNow = viewer.session;
  const applyFlag = useCallback(
    (flag: PhotoFlag | null, advance: boolean) => {
      const id = viewerPhotoIdNow();
      if (!id) return;
      const current = qc.getQueryData<PhotoDetails>(qk.photo(id));
      if (current) qc.setQueryData(qk.photo(id), { ...current, flag });
      mutateFlags.current({ selection: { kind: 'explicit', photoIds: [id] }, flag });
      const name = current?.filename ?? 'รูปนี้';
      toast.success(flag ? `${FLAG_LABEL[flag]} ${name}` : `ล้างการคัด ${name}`, { id: 'photo-flags' });
      const ids = sessionNow.current.ids;
      const at = ids.indexOf(id);
      const next = at >= 0 ? ids[at + 1] : undefined;
      if (advance && next) goTo(next);
    },
    [qc, goTo, sessionNow],
  );

  const close = useCallback(() => {
    if (state?.pushed) navigate(-1);
    else navigate({ pathname: state?.backgroundLocation.pathname ?? '/home', search: state?.backgroundLocation.search ?? '' }, { replace: true });
    window.setTimeout(() => viewer.restoreFocus(), 0);
  }, [state, navigate, viewer]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (isTypingTarget(event.target) || overlayOpen()) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const flag = flagForKey(event);
      if (flag !== undefined) {
        event.preventDefault();
        applyFlag(flag, flag !== null);
        return;
      }
      switch (event.key) {
        case 'ArrowLeft':
          event.preventDefault();
          goTo(previousId);
          break;
        case 'ArrowRight':
          event.preventDefault();
          goTo(nextId);
          break;
        case 'Escape':
          event.preventDefault();
          if (drawMode) setDrawMode(false);
          else close();
          break;
        case '+':
        case '=':
          event.preventDefault();
          stage.current?.zoomBy(1.4);
          break;
        case '-':
          event.preventDefault();
          stage.current?.zoomBy(1 / 1.4);
          break;
        case '0':
          event.preventDefault();
          stage.current?.fit();
          break;
        default:
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [goTo, previousId, nextId, close, drawMode, applyFlag]);

  const reveal = useMutation({
    mutationFn: () => call('reveal_photo', { photoId }),
    onError: (error) => toast.error(errorMessage(error)),
  });
  const openOriginal = useMutation({
    mutationFn: () => call('open_photo', { photoId }),
    onError: (error) => toast.error(errorMessage(error)),
  });
  const setCover = useMutation({
    mutationFn: () => call('set_event_cover', { eventId: data?.eventId as Id, photoId }),
    onSuccess: (event) => {
      void qc.invalidateQueries({ queryKey: ['events'] });
      toast.success(`ใช้รูปนี้เป็นภาพปกของ ${event.title} แล้ว (ไม่เขียนข้อมูลลงไฟล์ต้นฉบับ)`);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  const filmStart = Math.max(0, index - FILMSTRIP_RADIUS);
  const film = index >= 0 ? ids.slice(filmStart, index + FILMSTRIP_RADIUS + 1) : [];

  return (
    <div role="dialog" aria-modal="false" aria-label="ตัวดูภาพ" data-testid="photo-viewer" className="anim-fade absolute inset-0 z-30 flex flex-col bg-canvas">
      <div data-tauri-drag-region className="flex min-h-[60px] shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-b border-line bg-surface px-4 py-2">
        <Button size="sm" icon={<ArrowLeft className="size-4" aria-hidden />} onClick={close} data-testid="viewer-close">
          {viewer.session.current.originLabel}
        </Button>
        <p className="hidden max-w-[240px] min-w-0 truncate text-sm font-semibold text-ink min-[1280px]:block" title={data?.filename}>
          {data?.filename ?? ' '}
        </p>
        {index >= 0 && ids.length > 1 && (
          <div className="flex items-center gap-1">
            <IconButton label="รูปก่อนหน้า (ลูกศรซ้าย)" size="sm" disabled={!previousId} onClick={() => goTo(previousId)} data-testid="viewer-prev" className="border border-line-strong">
              <ChevronLeft className="size-4" aria-hidden />
            </IconButton>
            <span className="min-w-[76px] text-center text-[13.5px] text-muted tabular-nums" aria-live="polite">
              {formatCount(index + 1)} / {formatCount(total)}
            </span>
            <IconButton label="รูปถัดไป (ลูกศรขวา)" size="sm" disabled={!nextId} onClick={() => goTo(nextId)} data-testid="viewer-next" className="border border-line-strong">
              <ChevronRight className="size-4" aria-hidden />
            </IconButton>
          </div>
        )}

        <div className="flex items-center gap-1" role="group" aria-label="คัดรูป">
          <IconButton
            label={data?.flag === 'pick' ? 'เลือกไว้แล้ว — กดอีกครั้งเพื่อล้าง (U)' : 'เลือกรูปนี้ (P)'}
            size="sm"
            disabled={!data}
            aria-pressed={data?.flag === 'pick'}
            onClick={() => applyFlag(data?.flag === 'pick' ? null : 'pick', false)}
            data-testid="viewer-flag-pick"
            className={cn('border', data?.flag === 'pick' ? 'border-ok bg-ok text-white hover:bg-ok hover:text-white' : 'border-line-strong')}
          >
            <Flag className="size-4" fill={data?.flag === 'pick' ? 'currentColor' : 'none'} aria-hidden />
          </IconButton>
          <IconButton
            label={data?.flag === 'reject' ? 'ไม่เอาแล้ว — กดอีกครั้งเพื่อล้าง (U)' : 'ไม่เอารูปนี้ (X) — ไฟล์ไม่ถูกลบ'}
            size="sm"
            disabled={!data}
            aria-pressed={data?.flag === 'reject'}
            onClick={() => applyFlag(data?.flag === 'reject' ? null : 'reject', false)}
            data-testid="viewer-flag-reject"
            className={cn('border', data?.flag === 'reject' ? 'border-ink bg-ink text-surface hover:bg-ink hover:text-surface' : 'border-line-strong')}
          >
            <Ban className="size-4" aria-hidden />
          </IconButton>
        </div>

        <div data-tauri-drag-region className="min-w-2 flex-1" />

        <button
          type="button"
          role="switch"
          aria-checked={showFaces}
          onClick={() => setShowFaces((v) => !v)}
          data-testid="viewer-toggle-faces"
          className={cn(
            'inline-flex h-8 items-center gap-2 rounded-lg border px-2.5 text-[13px] font-semibold',
            showFaces ? 'border-brand-line bg-brand-soft text-action' : 'border-line-strong bg-surface text-muted',
          )}
        >
          <ScanFace className="size-4" aria-hidden />
          แสดงใบหน้า
          <span className={cn('relative h-4 w-7 rounded-full transition-colors', showFaces ? 'bg-brand' : 'bg-line-strong')} aria-hidden>
            <span className={cn('absolute top-0.5 size-3 rounded-full bg-white transition-all', showFaces ? 'left-3.5' : 'left-0.5')} />
          </span>
        </button>

        <div className="flex items-center rounded-lg border border-line-strong">
          <IconButton label="ย่อ (−)" size="sm" onClick={() => stage.current?.zoomBy(1 / 1.4)} disabled={zoom <= 1.001}>
            <Minus className="size-4" aria-hidden />
          </IconButton>
          <button
            type="button"
            onClick={() => stage.current?.fit()}
            title="พอดีหน้าต่าง (0)"
            className="h-8 min-w-[96px] border-x border-line-strong px-2 text-[13px] font-semibold text-ink hover:bg-sunken"
            data-testid="viewer-fit"
          >
            {zoom <= 1.001 ? 'พอดีหน้าต่าง' : `${Math.round(zoom * 100)}%`}
          </button>
          <IconButton label="ขยาย (+)" size="sm" onClick={() => stage.current?.zoomBy(1.4)}>
            <Plus className="size-4" aria-hidden />
          </IconButton>
        </div>

        <Button
          size="sm"
          variant="outline"
          icon={<SlidersHorizontal className="size-4" aria-hidden />}
          onClick={() => setEditing(true)}
          disabled={!data || data.width <= 0 || !data.originalAvailable}
          title={data && !data.originalAvailable ? 'แต่งภาพได้เมื่อไฟล์ต้นฉบับพร้อม' : undefined}
          data-testid="viewer-edit"
        >
          แต่งภาพ
        </Button>

        <Button size="sm" variant="outline" icon={<Upload className="size-4" aria-hidden />} onClick={() => setExporting(true)} disabled={!data} data-testid="viewer-export">
          ส่งออกสำเนา
        </Button>

        <Menu>
          <MenuTrigger asChild>
            <IconButton label="คำสั่งเพิ่มเติมของรูปนี้" size="sm" disabled={!data} className="border border-line-strong">
              <EllipsisVertical className="size-4" aria-hidden />
            </IconButton>
          </MenuTrigger>
          <MenuContent>
            <MenuItem icon={<FolderOpen />} disabled={!data?.originalAvailable} onSelect={() => reveal.mutate()}>
              เปิดใน {FILE_MANAGER}
            </MenuItem>
            <MenuItem icon={<ExternalLink />} disabled={!data?.originalAvailable} onSelect={() => openOriginal.mutate()}>
              เปิดต้นฉบับด้วยแอปของระบบ
            </MenuItem>
            <MenuItem icon={<ImageIcon />} onSelect={() => setCover.mutate()}>
              ใช้เป็นภาพปกกิจกรรม
            </MenuItem>
            <MenuItem
              icon={<ScanFace />}
              onSelect={() => {
                setShowFaces(true);
                setDrawMode(true);
              }}
            >
              เพิ่มกรอบใบหน้า
            </MenuItem>
            <MenuItem
              icon={<Copy />}
              disabled={!data?.edit}
              onSelect={() => {
                if (!data?.edit) return;
                clipboard.copy(data.edit.settings, data.filename);
                toast.success('คัดลอกการแต่งแล้ว เลือกรูปอื่นแล้วกด “วางการแต่ง”');
              }}
            >
              คัดลอกการแต่ง
            </MenuItem>
            <MenuItem icon={<ClipboardPaste />} disabled={!clipboard.copied} onSelect={() => setPasting(true)}>
              วางการแต่ง
            </MenuItem>
            <MenuItem icon={<Sparkles />} onSelect={() => setGenerating(true)}>
              สร้างภาพด้วย AI…
            </MenuItem>
            <MenuItem
              icon={<Palette />}
              disabled={!data}
              onSelect={() => {
                if (!data) return;
                useAssistant.getState().setReference({ reference: { kind: 'photo', photoId }, label: data.filename });
                useUi.getState().setAssistantOpen(true);
                toast.success('ตั้งเป็นภาพตัวอย่างในผู้ช่วย AI แล้ว — บอก AI ได้เลยว่าจะให้แต่งรูปไหนตามภาพนี้');
              }}
            >
              ใช้เป็นภาพตัวอย่างในผู้ช่วย AI
            </MenuItem>
            {data?.edit && (
              <MenuItem icon={<Undo2 />} onSelect={() => setConfirmRevert(true)} data-testid="viewer-revert">
                กลับเป็นภาพต้นฉบับ
              </MenuItem>
            )}
          </MenuContent>
        </Menu>

        <IconButton
          label={inspectorOpen ? 'ซ่อนรายละเอียดภาพ' : 'แสดงรายละเอียดภาพ'}
          size="sm"
          aria-pressed={inspectorOpen}
          onClick={() => setInspectorOpen((v) => !v)}
          className={cn('border', inspectorOpen ? 'border-brand-line bg-brand-soft text-action' : 'border-line-strong')}
        >
          <PanelRight className="size-4" aria-hidden />
        </IconButton>
        <IconButton label="ปิดตัวดูภาพ (Esc)" size="sm" onClick={close} data-testid="viewer-close-x">
          <X className="size-5" aria-hidden />
        </IconButton>
      </div>

      <div className="flex min-h-0 flex-1 gap-3 p-3">
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-2xl bg-viewer">
          <div className="relative flex min-h-0 flex-1">
            <ViewerStage
              ref={stage}
              photoId={photoId}
              details={data}
              showFaces={showFaces}
              drawMode={drawMode}
              onDrawModeChange={setDrawMode}
              selectedFaceId={selectedFaceId}
              onSelectFace={setSelectedFaceId}
              onZoomChange={setZoom}
            />
            {data?.flag && (
              // สถานะการคัดของรูปนี้ เห็นได้ทันทีระหว่างไล่คัด (DECISIONS ข้อ 33)
              <span
                data-testid="viewer-flag-state"
                className={cn(
                  'pointer-events-none absolute top-3 left-3 inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-[13px] font-semibold text-white shadow-card',
                  data.flag === 'pick' ? 'bg-ok' : 'bg-black/70',
                )}
              >
                {data.flag === 'pick' ? <Flag className="size-3.5" fill="currentColor" aria-hidden /> : <Ban className="size-3.5" aria-hidden />}
                {data.flag === 'pick' ? 'เลือกไว้' : 'ไม่เอา'}
              </span>
            )}
            {previousId && (
              <button
                type="button"
                aria-label="รูปก่อนหน้า"
                onClick={() => goTo(previousId)}
                className="absolute top-1/2 left-3 flex size-11 -translate-y-1/2 items-center justify-center rounded-full bg-black/45 text-white opacity-70 hover:bg-black/65 hover:opacity-100"
              >
                <ChevronLeft className="size-6" aria-hidden />
              </button>
            )}
            {nextId && (
              <button
                type="button"
                aria-label="รูปถัดไป"
                onClick={() => goTo(nextId)}
                className="absolute top-1/2 right-3 flex size-11 -translate-y-1/2 items-center justify-center rounded-full bg-black/45 text-white opacity-70 hover:bg-black/65 hover:opacity-100"
              >
                <ChevronRight className="size-6" aria-hidden />
              </button>
            )}
          </div>
          {film.length > 1 && (
            <ul aria-label="รูปใกล้เคียงในชุดนี้" className="flex shrink-0 justify-center gap-2 overflow-hidden px-3 pt-1 pb-3">
              {film.map((id) => (
                <li key={id} className="shrink-0">
                  <button
                    type="button"
                    aria-label={id === photoId ? 'รูปที่กำลังดู' : 'เปิดรูปนี้'}
                    aria-current={id === photoId || undefined}
                    onClick={() => goTo(id)}
                    className={cn('block h-[58px] w-[88px] overflow-hidden rounded-lg bg-white/5', id === photoId ? 'ring-[3px] ring-brand' : 'opacity-70 hover:opacity-100')}
                  >
                    <img
                      src={assetUrl('thumb', id, id === photoId && data ? (data.editId ?? data.revisionId) : undefined)}
                      alt=""
                      loading="lazy"
                      draggable={false}
                      className="size-full object-cover"
                      onError={(event) => (event.currentTarget.style.visibility = 'hidden')}
                    />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        {inspectorOpen && (
          <aside aria-label="รายละเอียดภาพ" className="w-[340px] max-w-[42%] shrink-0 overflow-hidden rounded-2xl border border-line bg-surface shadow-card">
            {details.isPending ? (
              <LoadingBlock label="กำลังโหลดรายละเอียดภาพ…" />
            ) : details.isError || !data ? (
              <ErrorState error={details.error} title="โหลดรายละเอียดภาพไม่สำเร็จ" onRetry={() => void details.refetch()} compact />
            ) : (
              <ViewerInspector
                details={data}
                selectedFaceId={selectedFaceId}
                onSelectFace={(id) => {
                  setSelectedFaceId(id);
                  if (id) setShowFaces(true);
                }}
                drawMode={drawMode}
                onDrawModeChange={(on) => {
                  setDrawMode(on);
                  if (on) setShowFaces(true);
                }}
                onReveal={() => reveal.mutate()}
                onOpenOriginal={() => openOriginal.mutate()}
              />
            )}
          </aside>
        )}
      </div>

      {data && <ExportDialog open={exporting} onOpenChange={setExporting} selection={{ kind: 'explicit', photoIds: [photoId] }} count={1} previewPhotoId={photoId} />}
      {editing && data && !(data.width > 0 && data.originalAvailable) && (
        // แต่งทีละรูปมาถึงรูปที่แต่งไม่ได้ตอนนี้: บอกเหตุ แล้วให้ไปต่อได้ ไม่ค้างเงียบ ๆ
        <div role="status" data-testid="viewer-edit-skip" className="absolute bottom-24 left-1/2 z-40 flex -translate-x-1/2 items-center gap-3 rounded-xl border border-line bg-surface px-4 py-2.5 text-sm text-ink shadow-card">
          {data.originalAvailable ? 'รูปนี้ยังเตรียมข้อมูลไม่เสร็จ จึงยังแต่งไม่ได้' : 'ไฟล์ต้นฉบับของรูปนี้ไม่พร้อม จึงแต่งไม่ได้ตอนนี้'}
          {editNext ? (
            <Button size="sm" variant="primary" icon={<ChevronRight className="size-4" aria-hidden />} onClick={() => editNext()}>
              แต่งรูปถัดไป
            </Button>
          ) : (
            <Button size="sm" onClick={() => setEditing(false)}>
              ตกลง
            </Button>
          )}
        </div>
      )}
      {editing && data && data.width > 0 && data.originalAvailable && (
        <PhotoEditor
          key={photoId}
          details={data}
          onClose={() => setEditing(false)}
          onSaved={applyEdited}
          sequence={index >= 0 && total > 1 ? { index, total, next: editNext } : undefined}
          initialTab={editorTab.current}
          onTabChange={(next) => (editorTab.current = next)}
        />
      )}
      <PasteEditDialog open={pasting} onOpenChange={setPasting} selection={{ kind: 'explicit', photoIds: [photoId] }} count={1} />
      <GenerateDialog open={generating} onOpenChange={setGenerating} selection={{ kind: 'explicit', photoIds: [photoId] }} count={1} />
      <ConfirmDialog
        open={confirmRevert}
        onOpenChange={setConfirmRevert}
        title="กลับเป็นภาพต้นฉบับ?"
        description="การตัด หมุน และปรับเอียงของรูปนี้จะถูกลบ ไฟล์ต้นฉบับไม่เคยถูกแตะ ใบหน้าและชื่อยังอยู่ครบ"
        confirmLabel="กลับเป็นต้นฉบับ"
        busy={revert.isPending}
        onConfirm={() => revert.mutate()}
      />
    </div>
  );
}
