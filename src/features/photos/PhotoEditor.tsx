import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Crop, Eye, Info, Palette, Pipette, RotateCcw, RotateCw, ScanFace, Sun, TriangleAlert, Wand2 } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { InlineNotice, Spinner } from '@/components/ui/Feedback';
import { Tabs } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { EditSettings, PhotoDetails, UnitRect } from '@/lib/ipc/types';
import { NEUTRAL, applyToPixels, autoAdjust, histogram, isNeutral, neutralize, type Adjust, type Histogram, type SliderKey } from './adjust';
import { ColorGradingPanel, HistogramView, HslPanel, LevelsPanel, SliderRow, ToneCurvePanel, VignettePanel } from './AdjustControls';
import { EditPresets } from './EditPresets';
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
type Tab = 'crop' | 'light' | 'presets';

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

interface SliderSpec {
  key: SliderKey;
  label: string;
  min: number;
  max: number;
  step: number;
  /** ข้อความปลายสองข้าง (ซ้าย, ขวา) */
  ends?: [string, string];
  format?: (v: number) => string;
}

const LIGHT: SliderSpec[] = [
  { key: 'exposure', label: 'ค่าแสง', min: -3, max: 3, step: 0.05, format: (v) => `${v > 0 ? '+' : ''}${v.toFixed(2)} EV` },
  { key: 'contrast', label: 'คอนทราสต์', min: -100, max: 100, step: 1 },
  { key: 'highlights', label: 'ส่วนสว่าง', min: -100, max: 100, step: 1 },
  { key: 'shadows', label: 'เงา', min: -100, max: 100, step: 1 },
  { key: 'whites', label: 'จุดขาว', min: -100, max: 100, step: 1 },
  { key: 'blacks', label: 'จุดดำ', min: -100, max: 100, step: 1 },
];

const COLOR: SliderSpec[] = [
  { key: 'temperature', label: 'อุณหภูมิสี', min: -100, max: 100, step: 1, ends: ['เย็น', 'อุ่น'] },
  { key: 'tint', label: 'โทนสี', min: -100, max: 100, step: 1, ends: ['เขียว', 'ม่วงแดง'] },
  { key: 'vibrance', label: 'ความสดของสี', min: -100, max: 100, step: 1 },
  { key: 'saturation', label: 'ความอิ่มสี', min: -100, max: 100, step: 1 },
];

const sameSettings = (a: EditSettings, b: EditSettings) => JSON.stringify(a) === JSON.stringify(b);

/** ภาพต้นฉบับย่อไว้สำหรับวาดตัวอย่าง (ด้านยาวไม่เกิน 2400 px — วาดซ้ำได้เร็วทุกครั้งที่เลื่อนแถบ) */
function useSource(photoId: string) {
  const [source, setSource] = useState<HTMLCanvasElement | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let alive = true;
    setSource(null);
    setFailed(false);
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.decoding = 'async';
    img.onload = () => {
      if (!alive) return;
      const scale = Math.min(1, 2400 / Math.max(img.naturalWidth, img.naturalHeight));
      const canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
      canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
      const ctx = canvas.getContext('2d');
      if (!ctx) {
        setFailed(true);
        return;
      }
      ctx.imageSmoothingQuality = 'high';
      ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
      setSource(canvas);
    };
    img.onerror = () => {
      if (alive) setFailed(true);
    };
    img.src = assetUrl('original', photoId);
    return () => {
      alive = false;
    };
  }, [photoId]);
  return { source, failed };
}

/**
 * วาดภาพที่แต่งเฉพาะส่วน `view` (สัดส่วนของกรอบหลังปรับเอียง) ลง canvas ทั้งผืน
 * `frame` = canvas นี้คือกรอบหลังตัดทั้งกรอบ (คิด Vignette ได้) · คืนฮิสโตแกรมของภาพที่วาดเมื่อขอ
 */
function paint(canvas: HTMLCanvasElement, source: HTMLCanvasElement, s: EditSettings, view: UnitRect, withAdjust: boolean, frame = false, wantHistogram = false): Histogram | null {
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) return null;
  const r = rotatedSize(source.width, source.height, s.rotate);
  const frameW = canvas.width / view.w;
  const frameH = canvas.height / view.h;
  const k = frameW / (frameScale(r.w, r.h, s.straighten) * r.w);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.translate(-view.x * frameW + frameW / 2, -view.y * frameH + frameH / 2);
  ctx.rotate(((s.rotate + s.straighten) * Math.PI) / 180);
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(source, (-source.width * k) / 2, (-source.height * k) / 2, source.width * k, source.height * k);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const adjusting = withAdjust && !isNeutral(s.adjust);
  if (!adjusting && !wantHistogram) return null;
  const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height);
  if (adjusting) {
    applyToPixels(pixels.data, s.adjust, frame ? canvas.width : undefined);
    ctx.putImageData(pixels, 0, 0);
  }
  return wantHistogram ? histogram(pixels.data) : null;
}

/** ค่าเฉลี่ยของสีรอบจุด (fx, fy) เป็นสัดส่วนของ canvas — รัศมี 2 พิกเซล */
function sampleAround(canvas: HTMLCanvasElement, fx: number, fy: number): [number, number, number] | null {
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) return null;
  const x = Math.min(Math.max(Math.round(fx * canvas.width) - 2, 0), canvas.width - 5);
  const y = Math.min(Math.max(Math.round(fy * canvas.height) - 2, 0), canvas.height - 5);
  const d = ctx.getImageData(x, y, 5, 5).data;
  const sum = [0, 0, 0];
  for (let i = 0; i < d.length; i += 4) for (let c = 0; c < 3; c += 1) sum[c]! += d[i + c]!;
  return sum.map((v) => v / (d.length / 4)) as [number, number, number];
}

interface PhotoEditorProps {
  details: PhotoDetails;
  onClose: () => void;
  onSaved: (details: PhotoDetails) => void;
}

/**
 * แต่งภาพ — ตัด หมุน ปรับเอียง แสงและสี (DECISIONS ข้อ 17–18)
 * เก็บเป็นค่าในคลัง ไฟล์ต้นฉบับไม่ถูกแตะ ใบหน้าและชื่ออยู่ครบเสมอ กลับเป็นต้นฉบับได้ทุกเมื่อ
 */
export function PhotoEditor({ details, onClose, onSaved }: PhotoEditorProps) {
  const saved = details.edit?.settings ?? IDENTITY;
  const [settings, setSettings] = useState<EditSettings>(saved);
  const [tab, setTab] = useState<Tab>('crop');
  const [aspect, setAspect] = useState<Aspect>('free');
  const [portrait, setPortrait] = useState(false);
  const [straightening, setStraightening] = useState(false);
  const [before, setBefore] = useState(false);
  const [hist, setHist] = useState<Histogram | null>(null);
  // หลอดดูดสี: คลิกจุดที่ควรเป็นสีขาว/เทา เพื่อตั้งอุณหภูมิสีและโทนสี
  const [picking, setPicking] = useState(false);
  // ชี้ที่ preset: ดูตัวอย่างบนภาพก่อนใช้
  const [previewAdjust, setPreviewAdjust] = useState<Adjust | null>(null);
  // ค่าที่วาดบนภาพ: ระหว่างชี้ที่ preset ใช้ค่าของ preset (memo ไว้ ไม่ให้วาดซ้ำทุกครั้งที่ฮิสโตแกรมอัปเดต)
  const shown = useMemo(() => (previewAdjust ? { ...settings, adjust: previewAdjust } : settings), [settings, previewAdjust]);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const { source, failed } = useSource(details.id);
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

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // กล่องหรือเมนูที่เปิดซ้อนบนหน้าแต่งภาพ (ตั้งชื่อ preset ฯลฯ) ปิดด้วย Esc ของมันเอง
      const nested = (event.target as Element | null)?.closest?.('[role="dialog"]:not([data-testid="photo-editor"]), [role="menu"]');
      if (event.key === 'Escape' && !confirmDiscard && !nested) {
        event.preventDefault();
        event.stopPropagation();
        // Esc ระหว่างใช้หลอดดูดสี = เลิกเลือกจุด ไม่ใช่ปิดหน้าแต่งภาพ
        if (picking) setPicking(false);
        else if (dirty) setConfirmDiscard(true);
        else onClose();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [dirty, confirmDiscard, onClose, picking]);

  const chooseAspect = (a: Aspect, upright = portrait) => {
    setAspect(a);
    const next = ratioOf(a, upright);
    if (next) setSettings((s) => ({ ...s, crop: fitAspect(s.crop, next, frameAspect) }));
  };

  const rotate = (direction: 1 | -1) => {
    setSettings((s) => ({ ...s, rotate: turn(s.rotate, direction), crop: rotateCrop(s.crop, direction) }));
    if (aspect !== 'free' && aspect !== 'original') setPortrait((p) => !p);
  };

  const setAdjust = (patch: Partial<Adjust>) => setSettings((s) => ({ ...s, adjust: { ...s.adjust, ...patch } }));

  // ปรับอัตโนมัติจากภาพที่ตัดแล้ว (ย่อเล็กพอสำหรับนับค่าความสว่าง)
  const auto = () => {
    if (!source) return;
    const probe = document.createElement('canvas');
    const crop = settings.crop ?? FULL;
    const aspectNow = (frameAspect * crop.w) / crop.h;
    probe.width = aspectNow >= 1 ? 320 : Math.max(1, Math.round(320 * aspectNow));
    probe.height = aspectNow >= 1 ? Math.max(1, Math.round(320 / aspectNow)) : 320;
    paint(probe, source, settings, crop, false);
    const data = probe.getContext('2d')?.getImageData(0, 0, probe.width, probe.height).data;
    if (!data) return;
    const suggested = autoAdjust(data);
    setAdjust({ exposure: suggested.exposure, whites: suggested.whites, blacks: suggested.blacks });
  };

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
          รีเซ็ตทั้งหมด
        </Button>
        <Button size="sm" onClick={() => (dirty ? setConfirmDiscard(true) : onClose())} data-testid="editor-cancel">
          ยกเลิก
        </Button>
        <Button size="sm" variant="primary" disabled={!dirty} busy={save.isPending} onClick={() => save.mutate()} data-testid="editor-save">
          บันทึก
        </Button>
      </div>

      <div className="flex min-h-0 flex-1 gap-3 p-3">
        <EditCanvas
          source={source}
          failed={failed}
          settings={shown}
          frameAspect={frameAspect}
          ratio={ratio}
          mode={tab}
          straightening={straightening}
          before={before}
          faces={faces}
          onCrop={(next) => setSettings((s) => ({ ...s, crop: next }))}
          onHistogram={setHist}
          picking={picking}
          onPick={(fx, fy) => {
            setPicking(false);
            if (!source) return;
            // สีของจุดนั้นก่อนปรับแสงสี (วาดส่วนที่เห็นแบบไม่ปรับ แล้วเฉลี่ยรอบจุด)
            const probe = document.createElement('canvas');
            const crop = settings.crop ?? FULL;
            const aspectNow = (frameAspect * crop.w) / crop.h;
            probe.width = aspectNow >= 1 ? 800 : Math.max(1, Math.round(800 * aspectNow));
            probe.height = aspectNow >= 1 ? Math.max(1, Math.round(800 / aspectNow)) : 800;
            paint(probe, source, settings, crop, false);
            const rgb = sampleAround(probe, fx, fy);
            if (!rgb) return;
            setAdjust(neutralize(...rgb));
            toast.success('ตั้งสมดุลแสงขาวจากจุดที่เลือกแล้ว');
          }}
        />

        <aside aria-label="เครื่องมือแต่งภาพ" className="flex w-[312px] shrink-0 flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-card">
          <div className="border-b border-line px-3 pt-2">
            <Tabs<Tab>
              value={tab}
              onChange={(next) => {
                setPreviewAdjust(null);
                setPicking(false);
                setTab(next);
              }}
              items={[
                { value: 'crop', label: 'ตัดและหมุน' },
                { value: 'light', label: 'แสงและสี' },
                { value: 'presets', label: 'Preset' },
              ]}
              label="เครื่องมือแต่งภาพ"
              testId="editor-tab"
            />
          </div>
          <div className="scroll-thin min-h-0 flex-1 space-y-5 overflow-y-auto p-4">
            {tab === 'crop' ? (
              <>
                <section>
                  <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold text-ink">
                    <Crop className="size-4 text-muted" aria-hidden /> สัดส่วน
                  </h3>
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
                    <button
                      type="button"
                      className="font-semibold text-action hover:underline disabled:text-faint disabled:no-underline"
                      disabled={settings.straighten === 0}
                      onClick={() => setSettings((s) => ({ ...s, straighten: 0 }))}
                    >
                      ตั้งตรง
                    </button>
                    <span>เอียงขวา</span>
                  </div>
                </section>

                {cutCount > 0 && (
                  <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
                    ใบหน้า {formatCount(cutCount)} ใบจะอยู่นอกกรอบที่ตัด — ใบหน้าและชื่อยังอยู่ครบ ไม่ถูกลบ
                  </InlineNotice>
                )}
              </>
            ) : tab === 'light' ? (
              <>
                <HistogramView data={hist} />
                <div className="flex gap-1.5">
                  <Button size="sm" icon={<Wand2 className="size-4" aria-hidden />} onClick={auto} disabled={!source} className="flex-1" data-testid="editor-auto">
                    ปรับอัตโนมัติ
                  </Button>
                  <Button size="sm" variant="ghost" disabled={isNeutral(settings.adjust)} onClick={() => setAdjust(NEUTRAL)} data-testid="editor-reset-light">
                    รีเซ็ต
                  </Button>
                </div>
                <SliderGroup title="แสง" icon={<Sun className="size-4 text-muted" aria-hidden />} specs={LIGHT} adjust={settings.adjust} onChange={setAdjust} />
                <LevelsPanel adjust={settings.adjust} onChange={setAdjust} />
                <ToneCurvePanel adjust={settings.adjust} onChange={setAdjust} />
                <SliderGroup
                  title="สี"
                  icon={<Palette className="size-4 text-muted" aria-hidden />}
                  specs={COLOR}
                  adjust={settings.adjust}
                  onChange={setAdjust}
                  action={
                    <Button
                      size="sm"
                      variant={picking ? 'primary' : 'ghost'}
                      icon={<Pipette className="size-4" aria-hidden />}
                      disabled={!source}
                      onClick={() => setPicking((v) => !v)}
                      aria-pressed={picking}
                      data-testid="editor-wb-picker"
                    >
                      {picking ? 'คลิกจุดสีเทาบนภาพ' : 'หลอดดูดสี'}
                    </Button>
                  }
                />
                <HslPanel adjust={settings.adjust} onChange={setAdjust} />
                <ColorGradingPanel adjust={settings.adjust} onChange={setAdjust} />
                <VignettePanel adjust={settings.adjust} onChange={setAdjust} />
                <BeforeButton disabled={isNeutral(settings.adjust)} onHold={setBefore} />
              </>
            ) : (
              <>
                <EditPresets adjust={settings.adjust} onApply={(adjust) => setSettings((s) => ({ ...s, adjust }))} onPreview={setPreviewAdjust} />
                <BeforeButton disabled={isNeutral(settings.adjust)} onHold={setBefore} />
              </>
            )}

            <p className="text-[13px] text-muted tabular-nums" data-testid="editor-output-size">
              ขนาดภาพที่ได้ {formatCount(out.w)} × {formatCount(out.h)} พิกเซล
            </p>
            <p className="flex items-start gap-1.5 text-[13px] text-muted">
              <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
              เก็บการแต่งไว้ในแอป ไฟล์ต้นฉบับไม่ถูกแตะ กลับเป็นภาพต้นฉบับได้ทุกเมื่อ · สำเนาที่ส่งออกจะเป็นภาพที่แต่งแล้ว
            </p>
          </div>
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

function SliderGroup({
  title,
  icon,
  specs,
  adjust,
  onChange,
  action,
}: {
  title: string;
  icon?: React.ReactNode;
  specs: SliderSpec[];
  adjust: Adjust;
  onChange: (patch: Partial<Adjust>) => void;
  /** ปุ่มเสริมท้ายหัวข้อ */
  action?: React.ReactNode;
}) {
  return (
    <section>
      <div className="mb-2 flex min-h-8 items-center justify-between gap-2">
        <h3 className="flex items-center gap-1.5 text-sm font-semibold text-ink">
          {icon}
          {title}
        </h3>
        {action}
      </div>
      <div className="space-y-3">
        {specs.map((spec) => (
          <SliderRow
            key={spec.key}
            id={`editor-${spec.key}`}
            label={spec.label}
            value={adjust[spec.key]}
            min={spec.min}
            max={spec.max}
            step={spec.step}
            ends={spec.ends}
            format={spec.format}
            onChange={(v) => onChange({ [spec.key]: v })}
          />
        ))}
      </div>
    </section>
  );
}

/** กดค้างเพื่อดูภาพก่อนปรับแสงและสี (เมาส์ หรือ Space ค้าง) */
function BeforeButton({ disabled, onHold }: { disabled: boolean; onHold: (holding: boolean) => void }) {
  return (
    <button
      type="button"
      onPointerDown={() => onHold(true)}
      onPointerUp={() => onHold(false)}
      onPointerLeave={() => onHold(false)}
      onKeyDown={(e) => e.key === ' ' && onHold(true)}
      onKeyUp={() => onHold(false)}
      disabled={disabled}
      className="flex h-9 w-full items-center justify-center gap-1.5 rounded-lg border border-line-strong text-[13px] font-semibold text-ink hover:bg-sunken disabled:opacity-50"
      data-testid="editor-before"
    >
      <Eye className="size-4" aria-hidden /> กดค้างเพื่อดูก่อนปรับแสงและสี
    </button>
  );
}

interface EditCanvasProps {
  source: HTMLCanvasElement | null;
  failed: boolean;
  settings: EditSettings;
  frameAspect: number;
  ratio: number | null;
  mode: Tab;
  straightening: boolean;
  before: boolean;
  faces: { face: PhotoDetails['faces'][number]; box: UnitRect; cut: boolean }[];
  onCrop: (crop: UnitRect) => void;
  /** ฮิสโตแกรมของภาพที่แสดงหลังวาดแต่ละครั้ง */
  onHistogram: (h: Histogram | null) => void;
  /** โหมดหลอดดูดสี: คลิกบนภาพแล้วส่งตำแหน่ง (สัดส่วนของภาพที่แสดง) */
  picking: boolean;
  onPick: (fx: number, fy: number) => void;
}

/** ตัวอย่างภาพที่แต่ง: แท็บตัดและหมุนแสดงทั้งกรอบพร้อมกรอบตัดที่ลากได้ · แท็บแสงและสีแสดงเฉพาะส่วนที่ตัดพร้อมแสงสี */
function EditCanvas({ source, failed, settings, frameAspect, ratio, mode, straightening, before, faces, onCrop, onHistogram, picking, onPick }: EditCanvasProps) {
  const area = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const drag = useRef<{ handle: Handle; x: number; y: number; start: UnitRect } | null>(null);
  const crop = settings.crop ?? FULL;
  const view = mode === 'crop' ? FULL : crop;

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
  const viewAspect = (frameAspect * view.w) / view.h;
  const boxW = Math.max(0, Math.min(size.w - pad * 2, (size.h - pad * 2) * viewAspect));
  const boxH = viewAspect > 0 ? boxW / viewAspect : 0;

  // วาดใหม่ทุกครั้งที่ค่าเปลี่ยน (รวมเป็นครั้งเดียวต่อเฟรมจอ)
  const frame = useRef(0);
  const redraw = useCallback(() => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      const el = canvas.current;
      if (!el || !source || boxW === 0) return;
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const w = Math.round(boxW * dpr);
      const h = Math.round(boxH * dpr);
      if (el.width !== w || el.height !== h) {
        el.width = w;
        el.height = h;
      }
      onHistogram(paint(el, source, settings, view, !before, mode !== 'crop', true));
    });
  }, [source, settings, view, before, boxW, boxH, mode, onHistogram]);
  useEffect(() => {
    redraw();
    return () => cancelAnimationFrame(frame.current);
  }, [redraw]);

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
      {!source && (
        <div className="flex flex-col items-center gap-2 text-white/80">
          {failed ? <p className="text-sm">เปิดไฟล์ต้นฉบับไม่ได้ ตรวจว่าไดรฟ์ต่ออยู่แล้วลองใหม่</p> : <Spinner className="size-6" />}
        </div>
      )}
      {source && boxW > 0 && (
        <div className="relative" style={{ width: boxW, height: boxH }} onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
          <canvas ref={canvas} className="absolute inset-0 size-full" aria-label="ตัวอย่างภาพที่แต่ง" data-testid="editor-preview" />
          {picking && mode !== 'crop' && (
            <button
              type="button"
              aria-label="คลิกจุดที่ควรเป็นสีขาวหรือสีเทา"
              className="absolute inset-0 cursor-crosshair"
              onClick={(e) => {
                const r = e.currentTarget.getBoundingClientRect();
                onPick((e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height);
              }}
              data-testid="editor-wb-target"
            />
          )}
          {mode === 'crop' && (
            <>
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
              <div aria-hidden className="pointer-events-none absolute inset-0 overflow-hidden">
                {faces.map(({ face, box, cut }) => (
                  <div
                    key={face.id}
                    className={cn('absolute rounded-md border-2', cut ? 'border-danger' : 'border-dashed border-white/80')}
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
            </>
          )}
        </div>
      )}
      {mode === 'crop' && faces.length > 0 && source && (
        <p className="pointer-events-none absolute bottom-3 left-3 flex items-center gap-1.5 rounded-md bg-black/65 px-2 py-1 text-[12.5px] text-white">
          <ScanFace className="size-3.5" aria-hidden /> กรอบประ = ใบหน้าในรูป · กรอบแดง = จะอยู่นอกกรอบตัด
        </p>
      )}
      {before && (
        <p className="pointer-events-none absolute top-3 left-1/2 -translate-x-1/2 rounded-full bg-black/70 px-3 py-1 text-[13px] font-semibold text-white">ก่อนปรับแสงและสี</p>
      )}
      {picking && mode !== 'crop' && (
        <p className="pointer-events-none absolute top-3 left-1/2 -translate-x-1/2 rounded-full bg-black/75 px-3 py-1 text-[13px] font-semibold text-white">
          คลิกจุดที่ควรเป็นสีขาวหรือสีเทา · Esc เพื่อยกเลิก
        </p>
      )}
    </div>
  );
}
