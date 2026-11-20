import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Info, RotateCcw, RotateCw, ScanFace, TriangleAlert } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { InlineNotice } from '@/components/ui/Feedback';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { EditSettings, PhotoDetails, UnitRect } from '@/lib/ipc/types';
import {
  FULL,
  IDENTITY,
  MAX_STRAIGHTEN,
  boxToFrame,
  cutOut,
  dragCrop,
  fitAspect,
  frameScale,
  isIdentity,
  outputSize,
  rotateCrop,
  rotatedSize,
  turn,
  type Handle,
} from './editGeometry';

type Aspect = 'free' | 'original' | '1:1' | '4:5' | '4:3' | '3:2' | '16:9';

const ASPECTS: { value: Aspect; label: string; ratio: number | null }[] = [
  { value: 'free', label: 'อิสระ', ratio: null },
  { value: 'original', label: 'ต้นฉบับ', ratio: null },
  { value: '1:1', label: '1:1', ratio: 1 },
  { value: '4:5', label: '4:5', ratio: 5 / 4 },
  { value: '4:3', label: '4:3', ratio: 4 / 3 },
  { value: '3:2', label: '3:2', ratio: 3 / 2 },
  { value: '16:9', label: '16:9', ratio: 16 / 9 },
];

const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];

const sameSettings = (a: EditSettings, b: EditSettings) => JSON.stringify(a) === JSON.stringify(b);

interface PhotoEditorProps {
  details: PhotoDetails;
  onClose: () => void;
  onSaved: (details: PhotoDetails) => void;
}

/**
 * แต่งภาพ — ตัด หมุน ปรับเอียง (DECISIONS ข้อ 17)
 * เก็บเป็นค่าในคลัง ไฟล์ต้นฉบับไม่ถูกแตะ ใบหน้าและชื่ออยู่ครบเสมอ กลับเป็นต้นฉบับได้ทุกเมื่อ
 */
export function PhotoEditor({ details, onClose, onSaved }: PhotoEditorProps) {
  const saved = details.edit?.settings ?? IDENTITY;
  const [settings, setSettings] = useState<EditSettings>(saved);
  const [aspect, setAspect] = useState<Aspect>('free');
  const [portrait, setPortrait] = useState(false);
  const [straightening, setStraightening] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const dirty = !sameSettings(settings, saved);

  const W = details.width;
  const H = details.height;
  const r = rotatedSize(W, H, settings.rotate);
  const frameAspect = r.w / r.h;
  const out = outputSize(W, H, settings);

  const ratioOf = (a: Aspect, upright = portrait): number | null => {
    if (a === 'original') return frameAspect;
    const base = ASPECTS.find((x) => x.value === a)?.ratio ?? null;
    return base && upright ? 1 / base : base;
  };
  const ratio = ratioOf(aspect);

  const save = useMutation({
    mutationFn: () => call('save_photo_edit', { photoId: details.id, settings }),
    onSuccess: (result) => {
      onSaved(result);
      toast.success(result.edit ? 'บันทึกการแต่งแล้ว ไฟล์ต้นฉบับไม่ถูกแตะ' : 'กลับเป็นภาพต้นฉบับแล้ว');
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const cancel = () => (dirty ? setConfirmDiscard(true) : onClose());

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !confirmDiscard) {
        event.preventDefault();
        event.stopPropagation();
        if (dirty) setConfirmDiscard(true);
        else onClose();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [dirty, confirmDiscard, onClose]);

  const chooseAspect = (a: Aspect, upright = portrait) => {
    setAspect(a);
    const next = ratioOf(a, upright);
    if (next) setSettings((s) => ({ ...s, crop: fitAspect(s.crop, next, frameAspect) }));
  };

  const rotate = (direction: 1 | -1) => {
    setSettings((s) => ({ ...s, rotate: turn(s.rotate, direction), crop: rotateCrop(s.crop, direction) }));
    // สัดส่วนที่ล็อกไว้หมุนตามภาพ
    if (aspect !== 'free' && aspect !== 'original') setPortrait((p) => !p);
  };

  // ใบหน้าบนกรอบหลังปรับเอียง: ใบไหนจะถูกตัดออก
  const faces = details.faces.map((face) => {
    const box = boxToFrame(W, H, settings, face.sourceBbox);
    return { face, box, cut: cutOut(box, settings.crop) };
  });
  const cutCount = faces.filter((f) => f.cut).length;

  return (
    <div role="dialog" aria-modal="true" aria-label="แต่งภาพ" data-state="open" data-testid="photo-editor" className="anim-fade absolute inset-0 z-40 flex flex-col bg-canvas">
      <div data-tauri-drag-region className="flex min-h-[60px] shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-b border-line bg-surface px-4 py-2">
        <h2 className="text-base font-bold text-ink">แต่งภาพ</h2>
        <p className="min-w-0 truncate text-sm text-muted" title={details.filename}>
          {details.filename}
        </p>
        <div data-tauri-drag-region className="min-w-2 flex-1" />
        <Button size="sm" variant="ghost" disabled={isIdentity(settings)} onClick={() => setSettings(IDENTITY)} data-testid="editor-reset">
          รีเซ็ต
        </Button>
        <Button size="sm" onClick={cancel} data-testid="editor-cancel">
          ยกเลิก
        </Button>
        <Button size="sm" variant="primary" disabled={!dirty} busy={save.isPending} onClick={() => save.mutate()} data-testid="editor-save">
          บันทึก
        </Button>
      </div>

      <div className="flex min-h-0 flex-1 gap-3 p-3">
        <CropCanvas
          photoId={details.id}
          width={W}
          height={H}
          settings={settings}
          frameAspect={frameAspect}
          ratio={ratio}
          straightening={straightening}
          faces={faces}
          onCrop={(next) => setSettings((s) => ({ ...s, crop: next }))}
        />

        <aside aria-label="เครื่องมือแต่งภาพ" className="scroll-thin w-[300px] shrink-0 space-y-5 overflow-y-auto rounded-2xl border border-line bg-surface p-4 shadow-card">
          <section>
            <h3 className="mb-2 text-sm font-semibold text-ink">สัดส่วน</h3>
            <div className="grid grid-cols-4 gap-1.5" role="radiogroup" aria-label="สัดส่วนกรอบตัด">
              {ASPECTS.map((a) => (
                <button
                  key={a.value}
                  type="button"
                  role="radio"
                  aria-checked={aspect === a.value}
                  onClick={() => chooseAspect(a.value)}
                  className={cn(
                    'h-8 rounded-lg border text-[13px] font-semibold transition-colors',
                    aspect === a.value ? 'border-action bg-action text-on-action' : 'border-line-strong bg-surface text-ink hover:bg-sunken',
                  )}
                  data-testid={`editor-aspect-${a.value}`}
                >
                  {a.label}
                </button>
              ))}
            </div>
            {aspect !== 'free' && aspect !== 'original' && aspect !== '1:1' && (
              <div className="mt-2 grid grid-cols-2 gap-1.5" role="radiogroup" aria-label="แนวของกรอบตัด">
                {[false, true].map((upright) => (
                  <button
                    key={String(upright)}
                    type="button"
                    role="radio"
                    aria-checked={portrait === upright}
                    onClick={() => {
                      setPortrait(upright);
                      chooseAspect(aspect, upright);
                    }}
                    className={cn(
                      'h-8 rounded-lg border text-[13px] font-semibold',
                      portrait === upright ? 'border-brand-line bg-brand-soft text-action' : 'border-line-strong text-ink hover:bg-sunken',
                    )}
                  >
                    {upright ? 'แนวตั้ง' : 'แนวนอน'}
                  </button>
                ))}
              </div>
            )}
          </section>

          <section>
            <h3 className="mb-2 text-sm font-semibold text-ink">หมุน</h3>
            <div className="grid grid-cols-2 gap-1.5">
              <Button size="sm" icon={<RotateCcw className="size-4" aria-hidden />} onClick={() => rotate(-1)} data-testid="editor-rotate-left">
                หมุนซ้าย
              </Button>
              <Button size="sm" icon={<RotateCw className="size-4" aria-hidden />} onClick={() => rotate(1)} data-testid="editor-rotate-right">
                หมุนขวา
              </Button>
            </div>
          </section>

          <section>
            <div className="mb-1 flex items-baseline justify-between">
              <label htmlFor="editor-straighten" className="text-sm font-semibold text-ink">
                ปรับเอียง
              </label>
              <span className="text-sm font-semibold text-action tabular-nums">{settings.straighten.toFixed(1)}°</span>
            </div>
            <input
              id="editor-straighten"
              type="range"
              min={-MAX_STRAIGHTEN}
              max={MAX_STRAIGHTEN}
              step={0.1}
              value={settings.straighten}
              onChange={(e) => setSettings((s) => ({ ...s, straighten: Number(e.target.value) }))}
              onPointerDown={() => setStraightening(true)}
              onPointerUp={() => setStraightening(false)}
              onBlur={() => setStraightening(false)}
              aria-valuetext={`${settings.straighten.toFixed(1)} องศา`}
              className="h-1.5 w-full cursor-pointer accent-[var(--action)]"
              data-testid="editor-straighten"
            />
            <div className="mt-1 flex justify-between text-[12.5px] text-muted">
              <span>เอียงซ้าย</span>
              <button type="button" className="font-semibold text-action hover:underline disabled:text-faint disabled:no-underline" disabled={settings.straighten === 0} onClick={() => setSettings((s) => ({ ...s, straighten: 0 }))}>
                ตั้งตรง
              </button>
              <span>เอียงขวา</span>
            </div>
          </section>

          <p className="text-[13px] text-muted tabular-nums" data-testid="editor-output-size">
            ขนาดภาพที่ได้ {formatCount(out.w)} × {formatCount(out.h)} พิกเซล
          </p>

          {cutCount > 0 && (
            <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
              ใบหน้า {formatCount(cutCount)} ใบจะอยู่นอกกรอบที่ตัด — ใบหน้าและชื่อยังอยู่ครบ ไม่ถูกลบ
            </InlineNotice>
          )}
          <p className="flex items-start gap-1.5 text-[13px] text-muted">
            <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
            เก็บการแต่งไว้ในแอป ไฟล์ต้นฉบับไม่ถูกแตะ กลับเป็นภาพต้นฉบับได้ทุกเมื่อ · สำเนาที่ส่งออกจะเป็นภาพที่แต่งแล้ว
          </p>
        </aside>
      </div>

      <ConfirmDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        title="ทิ้งการแต่งที่ยังไม่บันทึก?"
        description="ภาพจะกลับเป็นแบบที่บันทึกไว้ล่าสุด"
        confirmLabel="ทิ้งการแต่ง"
        cancelLabel="แต่งต่อ"
        tone="danger"
        onConfirm={() => {
          setConfirmDiscard(false);
          onClose();
        }}
      />
    </div>
  );
}

interface CropCanvasProps {
  photoId: string;
  width: number;
  height: number;
  settings: EditSettings;
  frameAspect: number;
  ratio: number | null;
  straightening: boolean;
  faces: { face: PhotoDetails['faces'][number]; box: UnitRect; cut: boolean }[];
  onCrop: (crop: UnitRect) => void;
}

/** ภาพหลังหมุนและปรับเอียงเต็มกรอบ พร้อมกรอบตัดที่ลากได้ (นอกกรอบมืดลง) */
function CropCanvas({ photoId, width, height, settings, frameAspect, ratio, straightening, faces, onCrop }: CropCanvasProps) {
  const area = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const drag = useRef<{ handle: Handle; x: number; y: number; start: UnitRect } | null>(null);

  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    const measure = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const pad = 40;
  const boxW = Math.max(0, Math.min(size.w - pad * 2, (size.h - pad * 2) * frameAspect));
  const boxH = boxW / frameAspect;
  const r = rotatedSize(width, height, settings.rotate);
  const k = boxW / (frameScale(r.w, r.h, settings.straighten) * r.w);
  const crop = settings.crop ?? FULL;
  const pct = (v: number) => `${v * 100}%`;

  const begin = (handle: Handle) => (event: React.PointerEvent) => {
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag.current = { handle, x: event.clientX, y: event.clientY, start: crop };
  };
  const move = (event: React.PointerEvent) => {
    const d = drag.current;
    if (!d || boxW === 0) return;
    onCrop(dragCrop(d.start, d.handle, (event.clientX - d.x) / boxW, (event.clientY - d.y) / boxH, ratio, frameAspect));
  };
  const end = () => {
    drag.current = null;
  };

  return (
    <div ref={area} className="relative flex min-w-0 flex-1 items-center justify-center overflow-hidden rounded-2xl bg-viewer" data-testid="editor-canvas">
      {boxW > 0 && (
        <div className="relative" style={{ width: boxW, height: boxH }} onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
          {/* ภาพที่หมุน/เอียงล้นกรอบ: ตัดขอบเฉพาะชั้นภาพ ชั้นกรอบตัดอยู่นอกชั้นนี้ ที่จับจึงไม่ถูกตัดครึ่ง */}
          <div className="absolute inset-0 overflow-hidden">
          <img
            src={assetUrl('original', photoId)}
            alt="ภาพที่กำลังแต่ง"
            draggable={false}
            className="pointer-events-none absolute top-1/2 left-1/2 max-w-none select-none"
            style={{
              width: width * k,
              height: height * k,
              transform: `translate(-50%, -50%) rotate(${settings.rotate + settings.straighten}deg)`,
            }}
          />
          {straightening && (
            <div
              aria-hidden
              className="pointer-events-none absolute inset-0"
              style={{
                backgroundImage: 'linear-gradient(rgba(255,255,255,0.28) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.28) 1px, transparent 1px)',
                backgroundSize: `${boxW / 12}px ${boxH / 8}px`,
              }}
            />
          )}
          {faces.map(({ face, box, cut }) => (
            <div
              key={face.id}
              aria-hidden
              className={cn('pointer-events-none absolute rounded-md border-2', cut ? 'border-danger' : 'border-white/80 border-dashed')}
              style={{ left: pct(box.x), top: pct(box.y), width: pct(box.w), height: pct(box.h) }}
            />
          ))}
          </div>
          <div
            role="group"
            aria-label="กรอบตัด ลากเพื่อย้าย ลากมุมหรือขอบเพื่อปรับขนาด"
            className="absolute cursor-move border-[1.5px] border-white shadow-[0_0_0_9999px_rgba(0,0,0,0.58)]"
            style={{ left: pct(crop.x), top: pct(crop.y), width: pct(crop.w), height: pct(crop.h) }}
            onPointerDown={begin('move')}
            data-testid="editor-crop"
          >
            {/* เส้นแบ่งสามส่วน */}
            <div aria-hidden className="pointer-events-none absolute inset-0">
              {[1, 2].map((i) => (
                <div key={`v${i}`} className="absolute top-0 bottom-0 w-px bg-white/45" style={{ left: `${(i * 100) / 3}%` }} />
              ))}
              {[1, 2].map((i) => (
                <div key={`h${i}`} className="absolute right-0 left-0 h-px bg-white/45" style={{ top: `${(i * 100) / 3}%` }} />
              ))}
            </div>
            {HANDLES.map((h) => (
              <span
                key={h}
                onPointerDown={begin(h)}
                data-testid={`editor-handle-${h}`}
                className={cn(
                  'absolute z-10 size-4 -translate-x-1/2 -translate-y-1/2 rounded-sm border-2 border-white bg-black/40',
                  h.length === 2 ? (h === 'nw' || h === 'se' ? 'cursor-nwse-resize' : 'cursor-nesw-resize') : h === 'n' || h === 's' ? 'cursor-ns-resize' : 'cursor-ew-resize',
                )}
                style={{ left: h.includes('w') ? '0%' : h.includes('e') ? '100%' : '50%', top: h.includes('n') ? '0%' : h.includes('s') ? '100%' : '50%' }}
              />
            ))}
          </div>
        </div>
      )}
      {faces.length > 0 && (
        <p className="pointer-events-none absolute bottom-3 left-3 flex items-center gap-1.5 rounded-md bg-black/65 px-2 py-1 text-[12.5px] text-white">
          <ScanFace className="size-3.5" aria-hidden /> กรอบประ = ใบหน้าในรูป · กรอบแดง = จะอยู่นอกกรอบตัด
        </p>
      )}
    </div>
  );
}
