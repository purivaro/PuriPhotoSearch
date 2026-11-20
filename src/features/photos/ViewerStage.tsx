import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from 'react';
import { CircleCheck, CircleHelp, CloudOff, ScanFace, Sparkles, X } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Popover, PopoverAnchor, PopoverContent } from '@/components/ui/Overlay';
import { LevelBadge } from '@/features/faces/LevelBadge';
import { PersonPicker, type PersonChoice } from '@/features/people/PersonPicker';
import { cn } from '@/lib/cn';
import { formatCount, newRequestId } from '@/lib/format';
import { assetUrl } from '@/lib/ipc/client';
import type { FaceBox, Id, PhotoDetails } from '@/lib/ipc/types';
import { boxFromDrag, clampPan, clampZoom, imageRect, isUsableBox, percent, toNormalized, zoomAround, type NormalizedBox, type Point, type Size } from './geometry';
import { useFaceMutations } from './useFaceMutations';

export interface ViewerStageHandle {
  zoomBy: (factor: number) => void;
  fit: () => void;
}

interface ViewerStageProps {
  photoId: Id;
  details: PhotoDetails | undefined;
  showFaces: boolean;
  drawMode: boolean;
  onDrawModeChange: (on: boolean) => void;
  selectedFaceId: Id | null;
  onSelectFace: (faceId: Id | null) => void;
  onZoomChange: (zoom: number) => void;
}

/**
 * พื้นที่ภาพของตัวดูภาพ: พอดีหน้าต่าง / zoom / เลื่อน และกรอบใบหน้า
 * กรอบวางเป็นเปอร์เซ็นต์ภายในกล่องเดียวกับภาพ จึงตรงทุกระดับ zoom และทุกขนาดหน้าต่าง
 * (พิกัดเป็นสัดส่วนบนภาพที่หมุนตาม EXIF แล้ว ซึ่งตรงกับที่ WebView วาด)
 */
export const ViewerStage = forwardRef<ViewerStageHandle, ViewerStageProps>(function ViewerStage(
  { photoId, details, showFaces, drawMode, onDrawModeChange, selectedFaceId, onSelectFace, onZoomChange },
  ref,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [container, setContainer] = useState<Size>({ w: 0, h: 0 });
  const [natural, setNatural] = useState<Size | null>(null);
  const [fullLoaded, setFullLoaded] = useState(false);
  const [fullFailed, setFullFailed] = useState(false);
  const [thumbFailed, setThumbFailed] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState<Point>({ x: 0, y: 0 });
  const [drag, setDrag] = useState<{ start: Point; current: Point } | null>(null);
  const [pending, setPending] = useState<NormalizedBox | null>(null);
  const [panning, setPanning] = useState(false);
  const panStart = useRef<{ pointer: Point; pan: Point } | null>(null);
  const requestId = useRef(newRequestId());
  const { createManual, assign, confirmSuggestion, rejectSuggestion } = useFaceMutations(photoId);
  // ใบหน้าที่ยังไม่มีชื่อซึ่งกำลังเปิดช่องตั้งชื่ออยู่ (คลิกกรอบแล้วตั้งชื่อได้ทันที ไม่ต้องลากกรอบเอง)
  const [namingFaceId, setNamingFaceId] = useState<Id | null>(null);

  const shown = details?.edit ? { w: details.edit.width, h: details.edit.height } : details ? { w: details.width, h: details.height } : null;
  const image: Size = natural ?? (shown && shown.w > 0 && shown.h > 0 ? shown : { w: 3, h: 2 });
  const rect = imageRect(container, image, zoom, pan);
  const view = useRef({ container, image, zoom, pan });
  view.current = { container, image, zoom, pan };

  // เปลี่ยนรูป (หรือแต่งรูปนี้ใหม่) = เริ่มมุมมองใหม่
  const editId = details?.editId ?? null;
  useEffect(() => {
    setNatural(null);
    setFullLoaded(false);
    setFullFailed(false);
    setThumbFailed(false);
    setZoom(1);
    setPan({ x: 0, y: 0 });
    setDrag(null);
    setPending(null);
    setNamingFaceId(null);
  }, [photoId, editId]);

  useEffect(() => onZoomChange(zoom), [zoom, onZoomChange]);

  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => setContainer({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // ขนาดหน้าต่างเปลี่ยน: ดึงภาพกลับเข้าขอบ
  useEffect(() => {
    setPan((current) => clampPan(container, image, zoom, current));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [container.w, container.h, image.w, image.h]);

  const applyZoom = useCallback((next: number, focus?: Point) => {
    const v = view.current;
    const point = focus ?? { x: v.container.w / 2, y: v.container.h / 2 };
    const result = zoomAround(v.container, v.image, v.zoom, v.pan, next, point);
    setZoom(result.zoom);
    setPan(result.pan);
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      zoomBy: (factor) => applyZoom(clampZoom(view.current.zoom * factor)),
      fit: () => {
        setZoom(1);
        setPan({ x: 0, y: 0 });
      },
    }),
    [applyZoom],
  );

  // wheel ต้องเป็น listener แบบ non-passive จึงจะกันหน้าเลื่อนได้
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const bounds = el.getBoundingClientRect();
      const factor = Math.exp(-event.deltaY * (event.ctrlKey ? 0.012 : 0.0022));
      applyZoom(view.current.zoom * factor, { x: event.clientX - bounds.left, y: event.clientY - bounds.top });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [applyZoom]);

  const localPoint = (event: React.PointerEvent): Point => {
    const bounds = containerRef.current?.getBoundingClientRect();
    return { x: event.clientX - (bounds?.left ?? 0), y: event.clientY - (bounds?.top ?? 0) };
  };

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || pending) return;
    const point = localPoint(event);
    if (drawMode) {
      const start = toNormalized(rect, point);
      event.currentTarget.setPointerCapture(event.pointerId);
      setDrag({ start, current: start });
      return;
    }
    onSelectFace(null);
    if (zoom > 1) {
      event.currentTarget.setPointerCapture(event.pointerId);
      panStart.current = { pointer: point, pan };
      setPanning(true);
    }
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const point = localPoint(event);
    if (drag) {
      setDrag({ start: drag.start, current: toNormalized(rect, point) });
    } else if (panStart.current) {
      const origin = panStart.current;
      setPan(clampPan(container, image, zoom, { x: origin.pan.x + point.x - origin.pointer.x, y: origin.pan.y + point.y - origin.pointer.y }));
    }
  };

  const onPointerUp = () => {
    if (drag) {
      const box = boxFromDrag(drag.start, drag.current);
      setDrag(null);
      if (isUsableBox(box)) {
        requestId.current = newRequestId();
        setPending(box);
      }
    }
    panStart.current = null;
    setPanning(false);
  };

  const original = Boolean(details?.originalAvailable) && !fullFailed;
  const draft = drag ? boxFromDrag(drag.start, drag.current) : pending;
  // ใบหน้าที่อยู่นอกกรอบตัดไม่วาดบนภาพ (ยังอยู่ในรายการคนในรูป)
  const faces = (details?.faces ?? []).filter((face) => !face.outsideCrop);
  const thumbVersion = details?.editId ?? details?.revisionId;

  return (
    <div
      ref={containerRef}
      data-testid="viewer-stage"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      className={cn(
        'relative min-h-0 flex-1 touch-none overflow-hidden select-none',
        drawMode ? 'cursor-crosshair' : zoom > 1 ? (panning ? 'cursor-grabbing' : 'cursor-grab') : 'cursor-default',
      )}
    >
      <div data-testid="viewer-image-box" className="absolute" style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}>
        {/* ภาพย่อแสดงทันที แล้วภาพต้นฉบับค่อยขึ้นทับเมื่อโหลดเสร็จ */}
        {!thumbFailed && (
          <img
            key={`thumb-${photoId}-${thumbVersion}`}
            src={assetUrl('thumb', photoId, thumbVersion)}
            alt=""
            draggable={false}
            onLoad={(event) => {
              const el = event.currentTarget;
              setNatural((current) => current ?? { w: el.naturalWidth, h: el.naturalHeight });
            }}
            onError={() => setThumbFailed(true)}
            className="absolute inset-0 size-full"
          />
        )}
        {details?.originalAvailable && !fullFailed && (
          <img
            key={`full-${photoId}-${thumbVersion}`}
            src={assetUrl('photo', photoId, thumbVersion)}
            alt={details.filename}
            draggable={false}
            decoding="async"
            onLoad={(event) => {
              const el = event.currentTarget;
              // ขนาดจริงหลังหมุนตาม EXIF ตามที่ WebView วาด
              setNatural({ w: el.naturalWidth, h: el.naturalHeight });
              setFullLoaded(true);
            }}
            onError={() => setFullFailed(true)}
            className={cn('absolute inset-0 size-full transition-opacity duration-150', fullLoaded ? 'opacity-100' : 'opacity-0')}
          />
        )}

        {showFaces &&
          faces.map((face) => (
            <FaceFrame
              key={face.id}
              face={face}
              selected={face.id === selectedFaceId}
              inert={drawMode || Boolean(pending)}
              naming={namingFaceId === face.id}
              busy={assign.isPending || confirmSuggestion.isPending || rejectSuggestion.isPending}
              onSelect={() => {
                onSelectFace(face.id);
                // ยังไม่มีชื่อที่ยืนยัน → เปิดช่องตั้งชื่อที่กรอบนี้เลย
                setNamingFaceId(face.person ? null : face.id);
              }}
              onNamingChange={(open) => setNamingFaceId(open ? face.id : null)}
              onPick={(choice) => assign.mutate({ faceId: face.id, choice }, { onSuccess: () => setNamingFaceId(null) })}
              onConfirmSuggestion={() => confirmSuggestion.mutate(face, { onSuccess: () => setNamingFaceId(null) })}
              onRejectSuggestion={() => rejectSuggestion.mutate(face)}
            />
          ))}

        {draft && (
          <Popover
            open={Boolean(pending)}
            onOpenChange={(open) => {
              if (!open && !createManual.isPending) setPending(null);
            }}
          >
            <PopoverAnchor asChild>
              <div
                data-testid="face-draft-box"
                className="pointer-events-none absolute rounded-lg border-[2.5px] border-dashed border-white bg-brand/15 shadow-[0_0_0_1.5px_rgba(0,0,0,0.55)]"
                style={{ left: percent(draft.x), top: percent(draft.y), width: percent(draft.w), height: percent(draft.h) }}
              />
            </PopoverAnchor>
            {pending && (
              <PopoverContent side="right" align="start" onPointerDown={(event) => event.stopPropagation()} aria-label="ใบหน้านี้คือใคร">
                <p className="border-b border-line px-3 pt-2.5 pb-2 text-sm font-bold text-ink">ใบหน้านี้คือใคร</p>
                <PersonPicker
                  busy={createManual.isPending}
                  onPick={(choice) =>
                    createManual.mutate(
                      { requestId: requestId.current, bbox: pending, choice },
                      {
                        onSuccess: () => {
                          setPending(null);
                          onDrawModeChange(false);
                        },
                      },
                    )
                  }
                />
              </PopoverContent>
            )}
          </Popover>
        )}
      </div>

      {thumbFailed && !original && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-white/80">
          <CloudOff className="size-10" aria-hidden />
          <p className="text-[15px] font-semibold">แสดงรูปนี้ไม่ได้ในตอนนี้</p>
          <p className="max-w-sm text-center text-sm text-white/65">ไม่พบทั้งไฟล์ต้นฉบับและภาพย่อ ลองเชื่อมต่อโฟลเดอร์ต้นทางแล้วเปิดใหม่ รูปถัดไปยังดูได้ตามปกติ</p>
        </div>
      )}

      {drawMode && !pending && (
        <div role="status" className="pointer-events-none absolute top-3 left-1/2 -translate-x-1/2 rounded-full bg-black/75 px-4 py-1.5 text-[13.5px] font-medium text-white">
          ลากกรอบรอบใบหน้าที่ต้องการ · กด Esc เพื่อยกเลิก
        </div>
      )}

      <div className="pointer-events-none absolute bottom-3 left-3 flex items-center gap-2">
        <span className="rounded-md bg-black/65 px-2 py-1 text-[12.5px] font-medium text-white" data-testid="viewer-resolution">
          {details
            ? original
              ? fullLoaded
                ? details.edit
                  ? `แต่งแล้ว ${formatCount(details.edit.width)} × ${formatCount(details.edit.height)}`
                  : `ต้นฉบับ ${formatCount(image.w)} × ${formatCount(image.h)}`
                : 'กำลังโหลดภาพต้นฉบับ…'
              : 'ภาพย่อ — ภาพต้นฉบับไม่พร้อม'
            : 'กำลังโหลดข้อมูลรูป…'}
        </span>
      </div>
    </div>
  );
});

interface FaceFrameProps {
  face: FaceBox;
  selected: boolean;
  inert: boolean;
  /** ช่องตั้งชื่อของกรอบนี้เปิดอยู่ (เฉพาะใบหน้าที่ยังไม่มีชื่อที่ยืนยัน) */
  naming: boolean;
  busy: boolean;
  onSelect: () => void;
  onNamingChange: (open: boolean) => void;
  onPick: (choice: PersonChoice) => void;
  onConfirmSuggestion: () => void;
  onRejectSuggestion: () => void;
}

/**
 * กรอบใบหน้าบนภาพ มีสามสถานะที่ต่างกันทั้งรูปแบบเส้นและไอคอน (ไม่ใช่สีอย่างเดียว):
 * ยืนยันแล้ว = เส้นทึบ + เครื่องหมายถูก · AI เสนอ = เส้นประสีแบรนด์ + ไอคอนตามระดับ · ยังไม่ทราบชื่อ = เส้นประสีขาว + ไอคอนสแกน
 */
function FaceFrame({ face, selected, inert, naming, busy, onSelect, onNamingChange, onPick, onConfirmSuggestion, onRejectSuggestion }: FaceFrameProps) {
  const confirmed = Boolean(face.person);
  const suggestion = confirmed ? null : face.suggestion;
  const label = confirmed ? `${face.person?.displayName} · ยืนยันแล้ว` : suggestion ? `อาจเป็น “${suggestion.person.displayName}” — รอตรวจสอบ` : 'ยังไม่ทราบชื่อ';
  const frame = (
    <button
      type="button"
      data-testid="face-box"
      data-face-id={face.id}
      data-face-state={confirmed ? 'confirmed' : suggestion ? 'suggested' : 'unknown'}
      tabIndex={inert ? -1 : 0}
      aria-label={
        confirmed
          ? `ใบหน้าของ ${face.person?.displayName} ยืนยันแล้ว`
          : suggestion
            ? `ใบหน้าที่ AI เสนอว่าอาจเป็น ${suggestion.person.displayName} รอตรวจสอบ กดเพื่อยืนยันหรือตั้งชื่อ`
            : 'ใบหน้าที่ยังไม่ทราบชื่อ กดเพื่อตั้งชื่อ'
      }
      aria-pressed={selected}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => {
        event.stopPropagation();
        onSelect();
      }}
      className={cn(
        'absolute rounded-lg',
        inert && 'pointer-events-none',
        confirmed ? 'border-[3px] border-brand' : suggestion ? 'border-[2.5px] border-dashed border-brand' : 'border-[2.5px] border-dashed border-white',
        'shadow-[0_0_0_1.5px_rgba(0,0,0,0.5)]',
        selected && 'ring-4 ring-white/80',
      )}
      style={{ left: percent(face.bbox.x), top: percent(face.bbox.y), width: percent(face.bbox.w), height: percent(face.bbox.h) }}
    >
      <span
        className={cn(
          'absolute top-full left-1/2 mt-1.5 inline-flex max-w-[260px] -translate-x-1/2 items-center gap-1 rounded-full px-2.5 py-0.5 text-[12.5px] font-semibold whitespace-nowrap shadow-raised',
          confirmed ? 'bg-action text-on-action' : 'bg-white text-[#20242b]',
        )}
      >
        {confirmed ? (
          <CircleCheck className="size-3.5 shrink-0" aria-hidden />
        ) : suggestion ? (
          suggestion.level === 'close' ? (
            <Sparkles className="size-3.5 shrink-0 text-[#b54708]" aria-hidden />
          ) : (
            <CircleHelp className="size-3.5 shrink-0 text-[#b54708]" aria-hidden />
          )
        ) : (
          <ScanFace className="size-3.5 shrink-0" aria-hidden />
        )}
        <span className="truncate">{label}</span>
      </span>
    </button>
  );
  if (confirmed) return frame;
  return (
    <Popover open={naming && !inert} onOpenChange={onNamingChange}>
      <PopoverAnchor asChild>{frame}</PopoverAnchor>
      <PopoverContent side="right" align="start" onPointerDown={(event) => event.stopPropagation()} aria-label="ใบหน้านี้คือใคร" data-testid="face-name-popover">
        <p className="border-b border-line px-3 pt-2.5 pb-2 text-sm font-bold text-ink">ใบหน้านี้คือใคร</p>
        {suggestion && (
          <div className="border-b border-line px-3 py-2.5">
            <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[13.5px] text-ink">
              <span>
                AI เสนอ: <span className="font-bold">{suggestion.person.displayName}</span>
              </span>
              <LevelBadge level={suggestion.level} />
            </p>
            <div className="mt-2 flex gap-2">
              <Button size="sm" variant="primary" icon={<CircleCheck className="size-3.5" aria-hidden />} disabled={busy} onClick={onConfirmSuggestion} data-testid="face-suggestion-yes">
                ใช่ {suggestion.person.displayName}
              </Button>
              <Button size="sm" variant="danger" icon={<X className="size-3.5" aria-hidden />} disabled={busy} onClick={onRejectSuggestion} data-testid="face-suggestion-no">
                ไม่ใช่
              </Button>
            </div>
          </div>
        )}
        <PersonPicker busy={busy} excludeIds={suggestion ? [suggestion.person.id] : []} onPick={onPick} placeholder={suggestion ? 'เป็นคนอื่น — พิมพ์ชื่อเพื่อค้นหาหรือสร้าง' : undefined} />
      </PopoverContent>
    </Popover>
  );
}
