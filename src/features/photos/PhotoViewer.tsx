import { useCallback, useEffect, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useLocation, useNavigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { ArrowLeft, ChevronLeft, ChevronRight, EllipsisVertical, ExternalLink, FolderOpen, Image as ImageIcon, Minus, PanelRight, Plus, ScanFace, Upload, X } from 'lucide-react';
import { useViewer, type ViewerLocationState } from '@/app/navigation';
import { Button, IconButton } from '@/components/ui/Button';
import { ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { Menu, MenuContent, MenuItem, MenuTrigger } from '@/components/ui/Overlay';
import { ExportDialog } from '@/features/export/ExportDialog';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { Id } from '@/lib/ipc/types';
import { qk } from '@/lib/queries';
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

  const details = useQuery({ queryKey: qk.photo(photoId), queryFn: () => call('get_photo_details', { photoId }) });
  const data = details.data?.id === photoId ? details.data : undefined;

  const session = viewer.session.current;
  const ids = session.ids;
  const index = ids.indexOf(photoId);
  const total = Math.max(session.total ?? ids.length, ids.length);
  const previousId = index > 0 ? ids[index - 1] : undefined;
  const nextId = index >= 0 && index < ids.length - 1 ? ids[index + 1] : undefined;

  useEffect(() => {
    setDrawMode(false);
    setSelectedFaceId(null);
  }, [photoId]);

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

  const close = useCallback(() => {
    if (state?.pushed) navigate(-1);
    else navigate({ pathname: state?.backgroundLocation.pathname ?? '/home', search: state?.backgroundLocation.search ?? '' }, { replace: true });
    window.setTimeout(() => viewer.restoreFocus(), 0);
  }, [state, navigate, viewer]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (isTypingTarget(event.target) || overlayOpen()) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
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
  }, [goTo, previousId, nextId, close, drawMode]);

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
                    <img src={assetUrl('thumb', id)} alt="" loading="lazy" draggable={false} className="size-full object-cover" onError={(event) => (event.currentTarget.style.visibility = 'hidden')} />
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

      {data && <ExportDialog open={exporting} onOpenChange={setExporting} selection={{ kind: 'explicit', photoIds: [photoId] }} count={1} />}
    </div>
  );
}
