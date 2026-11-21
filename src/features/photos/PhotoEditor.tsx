import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Aperture, Crop, Eye, Info, Palette, Pipette, RotateCcw, RotateCw, ScanFace, Sparkle, Sun, TriangleAlert, Wand2 } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { InlineNotice, Spinner } from '@/components/ui/Feedback';
import { Tabs } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { call, renderPreview } from '@/lib/ipc/client';
import type { EditSettings, PhotoDetails, PreviewRequest, PreviewSource, RawPairInfo, UnitRect } from '@/lib/ipc/types';
import { NEUTRAL, histogram, isNeutral, type Adjust, type Histogram } from './adjust';
import { ColorGradingPanel, HistogramView, HslPanel, LevelsPanel, ToneCurvePanel, VignettePanel } from './AdjustControls';
import { EditPresets } from './EditPresets';
import { MaskPanel } from './MaskPanel';
import { COLOR, LIGHT, PRESENCE, SliderGroup } from './SliderGroup';
import {
  FULL,
  IDENTITY,
  MAX_STRAIGHTEN,
  boxToFrame,
  cutOut,
  dragCrop,
  fitAspect,
  isIdentity,
  outputSize,
  rotateCrop,
  rotatedSize,
  turn,
  type Handle,
} from './editGeometry';

type Aspect = 'free' | 'original' | '1:1' | '4:5' | '4:3' | '3:2' | '16:9';
type Tab = 'crop' | 'light' | 'masks' | 'presets';

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

// ไม่มี Mask กับรายการ Mask ว่าง (ค่าจาก Rust ไม่ส่งช่องที่ว่าง) ถือว่าเหมือนกัน
const comparable = (s: EditSettings) => JSON.stringify({ ...s, masks: s.masks?.length ? s.masks : undefined });
const sameSettings = (a: EditSettings, b: EditSettings) => comparable(a) === comparable(b);

/** ส่วนของ Lightroom ที่ทำตามไม่ได้ แบบย่อ: Mask หลายอันนับรวมเป็นหนึ่งรายการ */
export function unsupportedSummary(items: string[]): string {
  const masks = items.filter((i) => i.startsWith('Mask '));
  const rest = items.filter((i) => !i.startsWith('Mask '));
  return [...rest, ...(masks.length ? [`Mask ${formatCount(masks.length)} อัน`] : [])].join(' · ');
}

/** รูปที่มีไฟล์ RAW คู่ (DECISIONS ข้อ 26): แต่งจาก RAW หรือบอกเหตุที่ใช้ RAW ไม่ได้ */
function RawPairNotice({ pair, fromRaw }: { pair: RawPairInfo; fromRaw: boolean }) {
  if (fromRaw && !pair.problem) {
    const skipped = unsupportedSummary(pair.unsupported);
    return (
      <InlineNotice tone="brand" icon={<Aperture aria-hidden />}>
        <span data-testid="editor-raw-pair">
          แต่งจากไฟล์ RAW คู่ “{pair.filename}”
          {pair.lightroom ? ' · เริ่มจากค่าที่แต่งใน Lightroom ปรับให้สว่างและสีใกล้ JPEG ที่ส่งออกไว้' : ' · เริ่มจากภาพที่ใกล้ JPEG ของกล้อง'}
        </span>
        {skipped && <span className="mt-1 block text-[13px] opacity-90">ทำตาม Lightroom ไม่ได้: {skipped} — ภาพจึงต่างจาก JPEG เล็กน้อย</span>}
      </InlineNotice>
    );
  }
  if (!pair.problem) return null;
  return (
    <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
      <span data-testid="editor-raw-pair">{fromRaw ? pair.problem : `แต่งจาก JPEG — ${pair.problem}`}</span>
    </InlineNotice>
  );
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
  // รูปที่มีไฟล์ RAW คู่: แต่งจาก RAW — ค่าที่นับว่าไม่ได้แต่งคือค่าเริ่มต้นจาก Lightroom (DECISIONS ข้อ 26)
  const pair = details.rawPairInfo;
  const fromRaw = pair?.editFromRaw ?? false;
  const unedited = (fromRaw ? pair?.start : null) ?? IDENTITY;
  const saved = details.edit?.settings ?? unedited;
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
  // ปรับเฉพาะส่วน: บริเวณที่เลือก และบริเวณที่แสดงเป็นสีแดงทับ (กดค้าง / เพิ่งเพิ่ม)
  const [activeMask, setActiveMask] = useState<string | null>(null);
  const [showArea, setShowArea] = useState<string | null>(null);
  // ค่าที่วาดบนภาพ: ระหว่างชี้ที่ preset ใช้ค่าของ preset (memo ไว้ ไม่ให้วาดซ้ำทุกครั้งที่ฮิสโตแกรมอัปเดต)
  const shown = useMemo(() => (previewAdjust ? { ...settings, adjust: previewAdjust } : settings), [settings, previewAdjust]);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  // ภาพต้นทางของตัวอย่างสด (Rust ถอดเองแล้ววาดทุกเฟรม — DECISIONS ข้อ 30) · RAW: Rust คิดแสงสีด้วยสูตรของ RAW เอง
  const sourceKind: PreviewSource = fromRaw ? 'rawframe' : 'original';
  // ภาพแรกขึ้นแล้ว (ปุ่มที่ต้องใช้ภาพต้นทางกดได้)
  const [ready, setReady] = useState(false);
  const dirty = !sameSettings(settings, saved);
  const atStart = fromRaw ? sameSettings(settings, unedited) : isIdentity(settings);
  const lightAtStart = fromRaw ? JSON.stringify(settings.adjust) === JSON.stringify(unedited.adjust) : isNeutral(settings.adjust);

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

  // ปรับอัตโนมัติจากภาพที่ตัดแล้ว (Rust นับค่าความสว่างจากภาพต้นทาง)
  const auto = async () => {
    try {
      const s = await call('preview_auto_light', { photoId: details.id, source: sourceKind, settings });
      setAdjust({ exposure: s.exposure, whites: s.whites, blacks: s.blacks });
    } catch (e) {
      toast.error(errorMessage(e));
    }
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
        <Button size="sm" variant="ghost" disabled={atStart} onClick={() => setSettings(unedited)} data-testid="editor-reset">
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
          photoId={details.id}
          sourceKind={sourceKind}
          onReady={setReady}
          failMessage={fromRaw ? (pair?.problem ?? 'เปิดไฟล์ RAW คู่ไม่ได้ ตรวจว่าไดรฟ์ต่ออยู่แล้วลองใหม่') : undefined}
          settings={shown}
          frameAspect={frameAspect}
          ratio={ratio}
          mode={tab}
          straightening={straightening}
          before={before}
          faces={faces}
          onCrop={(next) => setSettings((s) => ({ ...s, crop: next }))}
          onHistogram={setHist}
          overlay={tab === 'masks' ? showArea : null}
          picking={picking}
          onPick={async (fx, fy) => {
            setPicking(false);
            // สีของจุดนั้นก่อนปรับแสงสี (Rust เฉลี่ยรอบจุดบนส่วนที่ตัดแล้ว)
            try {
              const wb = await call('preview_white_balance', { photoId: details.id, source: sourceKind, settings, x: fx, y: fy });
              setAdjust(wb);
              toast.success('ตั้งสมดุลแสงขาวจากจุดที่เลือกแล้ว');
            } catch (e) {
              toast.error(errorMessage(e));
            }
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
                { value: 'masks', label: 'เฉพาะส่วน' },
                { value: 'presets', label: 'Preset' },
              ]}
              label="เครื่องมือแต่งภาพ"
              testId="editor-tab"
            />
          </div>
          <div className="scroll-thin min-h-0 flex-1 space-y-5 overflow-y-auto p-4">
            {pair && <RawPairNotice pair={pair} fromRaw={fromRaw} />}
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
                  <Button size="sm" icon={<Wand2 className="size-4" aria-hidden />} onClick={() => void auto()} disabled={!ready} className="flex-1" data-testid="editor-auto">
                    ปรับอัตโนมัติ
                  </Button>
                  <Button size="sm" variant="ghost" disabled={lightAtStart} onClick={() => setAdjust(fromRaw ? unedited.adjust : NEUTRAL)} data-testid="editor-reset-light">
                    รีเซ็ต
                  </Button>
                </div>
                <SliderGroup title="แสง" icon={<Sun className="size-4 text-muted" aria-hidden />} specs={LIGHT} adjust={settings.adjust} onChange={setAdjust} />
                <SliderGroup title="รายละเอียด" icon={<Sparkle className="size-4 text-muted" aria-hidden />} specs={PRESENCE} adjust={settings.adjust} onChange={setAdjust} />
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
                      disabled={!ready}
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
            ) : tab === 'masks' ? (
              <MaskPanel
                masks={settings.masks ?? []}
                faces={details.faces}
                selected={activeMask}
                onSelect={setActiveMask}
                onChange={(masks) => setSettings((s) => ({ ...s, masks }))}
                onShowArea={setShowArea}
              />
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
  photoId: string;
  sourceKind: PreviewSource;
  /** ภาพแรกขึ้นแล้ว / ยังไม่ขึ้น */
  onReady: (ready: boolean) => void;
  /** ข้อความเมื่อเปิดภาพต้นทางไม่ได้ (ค่าเริ่มต้น: ไฟล์ต้นฉบับ) */
  failMessage?: string;
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
  /** แสดงบริเวณของ Mask นี้เป็นสีแดงทับ */
  overlay: string | null;
}

/** ตัวอย่างภาพที่แต่ง: แท็บตัดและหมุนแสดงทั้งกรอบพร้อมกรอบตัดที่ลากได้ · แท็บแสงและสีแสดงเฉพาะส่วนที่ตัดพร้อมแสงสี */
function EditCanvas({ photoId, sourceKind, onReady, failMessage, settings, frameAspect, ratio, mode, straightening, before, faces, onCrop, onHistogram, picking, onPick, overlay }: EditCanvasProps) {
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

  // ขอภาพจาก Rust ทุกครั้งที่ค่าเปลี่ยน — ค้างได้ครั้งละหนึ่งคำขอ ค่าล่าสุดแทนค่าที่ยังไม่ได้ส่ง (ลากแถบเร็ว ๆ ภาพไม่ตามหลังเป็นคิว)
  const [state, setState] = useState<'loading' | 'ready' | 'failed'>('loading');
  const [problem, setProblem] = useState<string | null>(null);
  const busy = useRef(false);
  const pending = useRef<PreviewRequest | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    // StrictMode ติดตั้ง/ถอด/ติดตั้งซ้ำ: ตั้งกลับทุกครั้งที่ติดตั้ง
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const send = useCallback(
    async (req: PreviewRequest) => {
      busy.current = true;
      try {
        const pixels = new Uint8ClampedArray(await renderPreview(req));
        const el = canvas.current;
        // ขนาด canvas เปลี่ยนระหว่างรอ = ภาพนี้ไม่ใช่ของขนาดนี้แล้ว (คำขอถัดไปมีขนาดใหม่)
        if (alive.current && el && el.width === req.width && el.height === req.height) {
          el.getContext('2d')?.putImageData(new ImageData(pixels, req.width, req.height), 0, 0);
          onHistogram(histogram(pixels));
          setState('ready');
          onReady(true);
        }
      } catch (e) {
        // ภาพแรกเปิดไม่ได้ = แจ้ง · หลังจากนั้นคงภาพเดิมไว้ (คำขอถัดไปมักใช้ได้)
        if (alive.current) {
          setProblem(errorMessage(e));
          setState((s) => (s === 'ready' ? s : 'failed'));
        }
      } finally {
        busy.current = false;
        const next = pending.current;
        pending.current = null;
        if (next && alive.current) void send(next);
      }
    },
    [onHistogram, onReady],
  );
  useEffect(() => {
    const el = canvas.current;
    if (!el || boxW === 0) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(1, Math.round(boxW * dpr));
    const h = Math.max(1, Math.round(boxH * dpr));
    if (el.width !== w || el.height !== h) {
      el.width = w;
      el.height = h;
    }
    const req: PreviewRequest = { photoId, source: sourceKind, settings, width: w, height: h, view, whole: mode !== 'crop', adjust: !before, show: overlay };
    if (busy.current) pending.current = req;
    else void send(req);
  }, [photoId, sourceKind, settings, view, before, boxW, boxH, mode, overlay, send]);

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
      {state !== 'ready' && (
        <div className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-2 px-6 text-center text-white/80">
          {state === 'failed' ? (
            <p className="text-sm" data-testid="editor-preview-failed">
              {failMessage ?? 'เปิดไฟล์ต้นฉบับไม่ได้ ตรวจว่าไดรฟ์ต่ออยู่แล้วลองใหม่'}
              {problem && failMessage === undefined && <span className="mt-1 block text-[12.5px] text-white/60">{problem}</span>}
            </p>
          ) : (
            <Spinner className="size-6" />
          )}
        </div>
      )}
      {boxW > 0 && (
        <div className={cn('relative', state !== 'ready' && 'invisible')} style={{ width: boxW, height: boxH }} onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
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
      {mode === 'crop' && faces.length > 0 && state === 'ready' && (
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
