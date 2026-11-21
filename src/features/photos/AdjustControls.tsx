import { useMemo, useRef, useState, type ReactNode } from 'react';
import { ChevronDown } from 'lucide-react';
import { Tabs } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import type { Levels, Wheel } from '@/lib/ipc/types';
import {
  GRADING_NEUTRAL,
  HUE_NAMES,
  HUE_SWATCHES,
  LEVELS_IDENTITY,
  NEUTRAL,
  SHARPEN_NEUTRAL,
  VIGNETTE_NEUTRAL,
  curveFunction,
  gradingIsActive,
  hasCurves,
  hasHsl,
  hasLevels,
  hasSharpen,
  hasVignette,
  type Adjust,
  type Histogram,
} from './adjust';

const signed = (v: number) => (v > 0 ? `+${v}` : `${v}`);
const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

interface SliderRowProps {
  id: string;
  label: ReactNode;
  value: number;
  min: number;
  max: number;
  step: number;
  /** ข้อความปลายสองข้าง (ซ้าย, ขวา) */
  ends?: [string, string];
  format?: (v: number) => string;
  /** ค่าที่ดับเบิลคลิกแล้วคืนให้ (ปกติ 0) */
  defaultValue?: number;
  /** พื้นหลังของแถบ (เช่น ไล่สีของเฉดสี) */
  track?: string;
  onChange: (value: number) => void;
}

/** แถบเลื่อนหนึ่งค่า — ดับเบิลคลิกชื่อหรือแถบเพื่อคืนค่าเริ่มต้นแบบ Lightroom · ปุ่มลูกศรเลื่อนทีละขั้น */
export function SliderRow({ id, label, value, min, max, step, ends, format, defaultValue = 0, track, onChange }: SliderRowProps) {
  const changed = value !== defaultValue;
  return (
    <div>
      <div className="flex items-baseline justify-between gap-2">
        <label
          htmlFor={id}
          className="flex min-w-0 cursor-default items-center gap-1.5 text-[13.5px] text-ink select-none"
          onDoubleClick={() => onChange(defaultValue)}
          title="ดับเบิลคลิกเพื่อคืนค่าเริ่มต้น"
        >
          {label}
        </label>
        <span className={cn('shrink-0 text-[13px] tabular-nums', changed ? 'font-semibold text-action' : 'text-muted')}>{format ? format(value) : signed(value)}</span>
      </div>
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        onDoubleClick={() => onChange(defaultValue)}
        className={cn('h-1.5 w-full cursor-pointer accent-[var(--action)]', track && 'appearance-none rounded-full')}
        style={track ? { background: track } : undefined}
        data-testid={id}
      />
      {ends && (
        <div className="flex justify-between text-[11.5px] text-muted">
          <span>{ends[0]}</span>
          <span>{ends[1]}</span>
        </div>
      )}
    </div>
  );
}

/** หัวข้อที่พับได้ — เปิดเองเมื่อส่วนนั้นมีค่าที่ปรับไว้ */
function Collapsible({ title, active, onReset, children, testId }: { title: string; active: boolean; onReset: () => void; children: ReactNode; testId: string }) {
  const [open, setOpen] = useState(active);
  return (
    <section className="rounded-xl border border-line">
      <div className="flex items-center gap-1 pr-2">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
          className="flex min-h-10 flex-1 items-center gap-1.5 rounded-xl px-3 text-left text-sm font-semibold text-ink"
          data-testid={`${testId}-toggle`}
        >
          <ChevronDown className={cn('size-4 text-muted transition-transform', !open && '-rotate-90')} aria-hidden />
          {title}
          {active && <span className="size-1.5 rounded-full bg-action" aria-label="มีค่าที่ปรับไว้" />}
        </button>
        {active && (
          <button type="button" onClick={onReset} className="rounded-md px-1.5 py-1 text-[12.5px] font-semibold text-action hover:bg-sunken" data-testid={`${testId}-reset`}>
            รีเซ็ต
          </button>
        )}
      </div>
      {open && <div className="space-y-3 border-t border-line px-3 pt-3 pb-3.5">{children}</div>}
    </section>
  );
}

type Channel = 'rgb' | 'red' | 'green' | 'blue';
const CHANNELS: { value: Channel; label: string; color: string }[] = [
  { value: 'rgb', label: 'RGB', color: 'var(--ink)' },
  { value: 'red', label: 'แดง', color: '#e5484d' },
  { value: 'green', label: 'เขียว', color: '#3fa34d' },
  { value: 'blue', label: 'น้ำเงิน', color: '#3b6fd8' },
];

/** ฮิสโตแกรมของภาพที่แสดง (หลังแต่ง) — ช่องแดง เขียว น้ำเงินซ้อนกัน แบบ Lightroom */
export function HistogramView({ data }: { data: Histogram | null }) {
  const paths = useMemo(() => {
    if (!data) return null;
    // ตัดค่าสุดขอบ (0 และ 255) ออกจากการหาความสูงสูงสุด ไม่ให้พิกเซลที่ดำ/ขาวสนิทกดกราฟจนแบน
    const peak = Math.max(1, ...[data.r, data.g, data.b].flatMap((c) => Array.from(c.subarray(1, 255))));
    const toPath = (c: Uint32Array) => {
      let d = 'M0,100';
      for (let i = 0; i < 256; i += 1) d += ` L${((i / 255) * 100).toFixed(2)},${(100 - Math.min(1, Math.sqrt(c[i]! / peak)) * 100).toFixed(2)}`;
      return `${d} L100,100 Z`;
    };
    return [
      { d: toPath(data.r), color: '#ff4d4d' },
      { d: toPath(data.g), color: '#3ddc5a' },
      { d: toPath(data.b), color: '#4d7dff' },
    ];
  }, [data]);
  return (
    <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="h-20 w-full rounded-lg bg-viewer" role="img" aria-label="ฮิสโตแกรมของภาพ" data-testid="editor-histogram">
      {paths?.map((p) => <path key={p.color} d={p.d} fill={p.color} fillOpacity={0.55} style={{ mixBlendMode: 'screen' }} />)}
    </svg>
  );
}

const LEVEL_KEYS: Record<Channel, 'levels' | 'levelsRed' | 'levelsGreen' | 'levelsBlue'> = { rgb: 'levels', red: 'levelsRed', green: 'levelsGreen', blue: 'levelsBlue' };

/** Levels แบบ Photoshop: รวมทุกช่อง หรือแยกช่องแดง/เขียว/น้ำเงิน */
export function LevelsPanel({ adjust, onChange }: { adjust: Adjust; onChange: (patch: Partial<Adjust>) => void }) {
  const [channel, setChannel] = useState<Channel>('rgb');
  const key = LEVEL_KEYS[channel];
  const l = adjust[key];
  const set = (patch: Partial<Levels>) => onChange({ [key]: { ...l, ...patch } });
  // แกมมาเลื่อนแบบลอการิทึม: กึ่งกลางแถบ = 1.00
  const gammaStep = Math.round(Math.log10(l.gamma) * 100);
  return (
    <Collapsible
      title="Levels"
      active={hasLevels(adjust)}
      onReset={() => onChange({ levels: LEVELS_IDENTITY, levelsRed: LEVELS_IDENTITY, levelsGreen: LEVELS_IDENTITY, levelsBlue: LEVELS_IDENTITY })}
      testId="editor-levels"
    >
      <Tabs<Channel>
        variant="pill"
        className="w-full [&>button]:flex-1"
        value={channel}
        onChange={setChannel}
        label="ช่องสีของ Levels"
        testId="editor-levels-channel"
        items={CHANNELS.map((c) => ({ value: c.value, label: c.label }))}
      />
      <p className="text-[12.5px] font-semibold text-muted">ค่าเข้า</p>
      <SliderRow
        id={`editor-levels-${channel}-inBlack`}
        label="จุดดำ"
        value={l.inBlack}
        min={0}
        max={253}
        step={1}
        format={(v) => `${v}`}
        onChange={(v) => set({ inBlack: clamp(v, 0, l.inWhite - 2) })}
      />
      <SliderRow
        id={`editor-levels-${channel}-gamma`}
        label="ช่วงกลาง (แกมมา)"
        value={gammaStep}
        min={-100}
        max={100}
        step={1}
        format={() => l.gamma.toFixed(2)}
        ends={['มืดลง', 'สว่างขึ้น']}
        onChange={(v) => set({ gamma: Math.round(clamp(10 ** (v / 100), 0.1, 9.99) * 100) / 100 })}
      />
      <SliderRow
        id={`editor-levels-${channel}-inWhite`}
        label="จุดขาว"
        value={l.inWhite}
        min={2}
        max={255}
        step={1}
        defaultValue={255}
        format={(v) => `${v}`}
        onChange={(v) => set({ inWhite: clamp(v, l.inBlack + 2, 255) })}
      />
      <p className="text-[12.5px] font-semibold text-muted">ค่าออก</p>
      <SliderRow
        id={`editor-levels-${channel}-outBlack`}
        label="ดำสุด"
        value={l.outBlack}
        min={0}
        max={254}
        step={1}
        format={(v) => `${v}`}
        onChange={(v) => set({ outBlack: clamp(v, 0, l.outWhite - 1) })}
      />
      <SliderRow
        id={`editor-levels-${channel}-outWhite`}
        label="ขาวสุด"
        value={l.outWhite}
        min={1}
        max={255}
        step={1}
        defaultValue={255}
        format={(v) => `${v}`}
        onChange={(v) => set({ outWhite: clamp(v, l.outBlack + 1, 255) })}
      />
    </Collapsible>
  );
}

/** ชื่อ Parametric curve เรียงจากสว่างลงมืดแบบ Lightroom (ค่าเก็บเรียงมืด → สว่าง) */
const REGIONS: { index: number; label: string }[] = [
  { index: 3, label: 'สว่างจัด' },
  { index: 2, label: 'สว่าง' },
  { index: 1, label: 'มืด' },
  { index: 0, label: 'มืดจัด' },
];

const CURVE_KEYS: Record<Channel, 'curve' | 'curveRed' | 'curveGreen' | 'curveBlue'> = { rgb: 'curve', red: 'curveRed', green: 'curveGreen', blue: 'curveBlue' };
const STRAIGHT: [number, number][] = [
  [0, 0],
  [255, 255],
];
const MAX_POINTS = 16;

/** เส้นโค้งแบบจุดที่ลากได้: คลิกที่ว่างเพื่อเพิ่มจุด ลากจุดเพื่อปรับ ดับเบิลคลิกจุดเพื่อลบ · เลือกจุดแล้วใช้ลูกศร/Delete ได้ */
function CurveEditor({ points, color, onChange }: { points: [number, number][]; color: string; onChange: (points: [number, number][]) => void }) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<number | null>(null);
  // ดับเบิลคลิกตรวจเองจาก pointerdown (preventDefault ของ pointerdown ทำให้บาง WebView ไม่ส่ง dblclick)
  const lastDown = useRef<{ index: number; at: number } | null>(null);
  const [active, setActive] = useState<number | null>(null);
  const pts = points.length >= 2 ? points : STRAIGHT;
  const path = useMemo(() => {
    const f = curveFunction(pts);
    let d = '';
    for (let x = 0; x <= 256; x += 4) {
      const xi = Math.min(x, 255);
      d += `${d ? ' L' : 'M'}${xi},${(255 - f(xi / 255) * 255).toFixed(2)}`;
    }
    return d;
  }, [pts]);

  const at = (event: React.PointerEvent) => {
    // viewBox เผื่อขอบ 4 หน่วยรอบกราฟ (กว้าง 263 เริ่มที่ −4) ให้จุดที่ปลายไม่ถูกตัด
    const r = svg.current!.getBoundingClientRect();
    const x = ((event.clientX - r.left) / r.width) * 263 - 4;
    const y = ((event.clientY - r.top) / r.height) * 263 - 4;
    return [clamp(Math.round(x), 0, 255), clamp(Math.round(255 - y), 0, 255)] as const;
  };
  /** ย้ายจุด i ไปที่ (x, y) โดยไม่ข้ามจุดข้างเคียง */
  const move = (list: [number, number][], i: number, x: number, y: number): [number, number][] => {
    const lo = i === 0 ? 0 : list[i - 1]![0] + 1;
    const hi = i === list.length - 1 ? 255 : list[i + 1]![0] - 1;
    return list.map((p, k) => (k === i ? [clamp(x, lo, hi), clamp(y, 0, 255)] : p));
  };

  const down = (event: React.PointerEvent<SVGSVGElement>) => {
    event.preventDefault();
    try {
      svg.current?.setPointerCapture(event.pointerId);
    } catch {
      /* ตัวชี้สังเคราะห์ */
    }
    const [x, y] = at(event);
    const hit = pts.findIndex(([px, py]) => Math.abs(px - x) <= 8 && Math.abs(py - y) <= 8);
    if (hit >= 0) {
      const now = event.timeStamp;
      if (lastDown.current?.index === hit && now - lastDown.current.at < 400) {
        lastDown.current = null;
        remove(hit);
        return;
      }
      lastDown.current = { index: hit, at: now };
      drag.current = hit;
      setActive(hit);
      return;
    }
    if (pts.length >= MAX_POINTS || pts.some(([px]) => px === x)) return;
    // เพิ่มจุดบนเส้นเดิม แล้วลากต่อได้ทันที
    const next = [...pts, [x, Math.round(curveFunction(pts)(x / 255) * 255)] as [number, number]].sort((a, b) => a[0] - b[0]);
    const index = next.findIndex(([px]) => px === x);
    drag.current = index;
    setActive(index);
    onChange(next);
  };
  const moveTo = (event: React.PointerEvent<SVGSVGElement>) => {
    if (drag.current === null) return;
    const [x, y] = at(event);
    onChange(move(pts, drag.current, x, y));
  };
  const remove = (i: number) => {
    if (i === 0 || i === pts.length - 1) return;
    onChange(pts.filter((_, k) => k !== i));
    setActive(null);
  };
  const key = (event: React.KeyboardEvent, i: number) => {
    const step = event.shiftKey ? 10 : 1;
    const [x, y] = pts[i]!;
    const delta: Record<string, [number, number]> = { ArrowUp: [0, step], ArrowDown: [0, -step], ArrowLeft: [-step, 0], ArrowRight: [step, 0] };
    if (delta[event.key]) {
      event.preventDefault();
      onChange(move(pts, i, x + delta[event.key]![0], y + delta[event.key]![1]));
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault();
      remove(i);
    }
  };

  return (
    <svg
      ref={svg}
      viewBox="-4 -4 263 263"
      className="aspect-square w-full touch-none rounded-lg border border-line bg-sunken"
      role="group"
      aria-label="เส้นโค้งโทน คลิกที่ว่างเพื่อเพิ่มจุด ลากจุดเพื่อปรับ ดับเบิลคลิกจุดเพื่อลบ"
      onPointerDown={down}
      onPointerMove={moveTo}
      onPointerUp={() => (drag.current = null)}
      onPointerCancel={() => (drag.current = null)}
      data-testid="editor-curve-graph"
    >
      {[64, 128, 192].map((v) => (
        <g key={v} stroke="var(--line)" strokeWidth="1">
          <line x1={v} y1="0" x2={v} y2="255" />
          <line x1="0" y1={v} x2="255" y2={v} />
        </g>
      ))}
      <line x1="0" y1="255" x2="255" y2="0" stroke="var(--line-strong)" strokeWidth="1" strokeDasharray="4 4" />
      <path d={path} fill="none" stroke={color} strokeWidth="2.5" strokeLinejoin="round" />
      {pts.map(([x, y], i) => (
        <circle
          key={i}
          cx={x}
          cy={255 - y}
          r={active === i ? 7 : 5.5}
          fill={active === i ? color : 'var(--surface)'}
          stroke={color}
          strokeWidth="2.5"
          tabIndex={0}
          role="slider"
          aria-label={`จุดที่ ${i + 1}`}
          aria-valuetext={`ค่าเข้า ${x} ค่าออก ${y}`}
          aria-valuenow={y}
          aria-valuemin={0}
          aria-valuemax={255}
          className="cursor-grab outline-none focus-visible:stroke-[4]"
          onFocus={() => setActive(i)}
          onKeyDown={(e) => key(e, i)}
          data-testid="editor-curve-point"
        />
      ))}
    </svg>
  );
}

export function ToneCurvePanel({ adjust, onChange }: { adjust: Adjust; onChange: (patch: Partial<Adjust>) => void }) {
  const [channel, setChannel] = useState<Channel>('rgb');
  const key = CURVE_KEYS[channel];
  const reset = () => onChange({ parametric: NEUTRAL.parametric, parametricSplit: null, curve: [], curveRed: [], curveGreen: [], curveBlue: [] });
  const point = adjust[key].length >= 2 ? adjust[key] : STRAIGHT;
  return (
    <Collapsible title="เส้นโค้งโทน" active={hasCurves(adjust)} onReset={reset} testId="editor-curve">
      <Tabs<Channel>
        variant="pill"
        className="w-full [&>button]:flex-1"
        value={channel}
        onChange={setChannel}
        label="ช่องสีของเส้นโค้ง"
        testId="editor-curve-channel"
        items={CHANNELS.map((c) => ({ value: c.value, label: c.label }))}
      />
      <CurveEditor key={channel} points={adjust[key]} color={CHANNELS.find((c) => c.value === channel)!.color} onChange={(points) => onChange({ [key]: points })} />
      <p className="text-[12.5px] text-muted tabular-nums">
        {point.length} จุด · คลิกที่ว่างเพื่อเพิ่ม ลากเพื่อปรับ ดับเบิลคลิกเพื่อลบ
      </p>
      <p className="pt-1 text-[12.5px] font-semibold text-muted">ปรับตามช่วงความสว่าง (Parametric)</p>
      {REGIONS.map(({ index, label }) => (
        <SliderRow
          key={index}
          id={`editor-parametric-${index}`}
          label={label}
          value={adjust.parametric[index] ?? 0}
          min={-100}
          max={100}
          step={1}
          onChange={(v) => onChange({ parametric: adjust.parametric.map((x, i) => (i === index ? v : x)) })}
        />
      ))}
    </Collapsible>
  );
}

type HslMode = 'hue' | 'sat' | 'lum';

export function HslPanel({ adjust, onChange }: { adjust: Adjust; onChange: (patch: Partial<Adjust>) => void }) {
  const [mode, setMode] = useState<HslMode>('lum');
  return (
    <Collapsible title="สีแยกช่วง (HSL)" active={hasHsl(adjust)} onReset={() => onChange({ hue: NEUTRAL.hue, sat: NEUTRAL.sat, lum: NEUTRAL.lum })} testId="editor-hsl">
      <Tabs<HslMode>
        variant="pill"
        className="w-full [&>button]:flex-1"
        value={mode}
        onChange={setMode}
        label="ส่วนของสีที่ปรับ"
        testId="editor-hsl-mode"
        items={[
          { value: 'hue', label: 'เฉดสี' },
          { value: 'sat', label: 'ความอิ่ม' },
          { value: 'lum', label: 'ความสว่าง' },
        ]}
      />
      {HUE_NAMES.map((name, i) => (
        <SliderRow
          key={`${mode}-${i}`}
          id={`editor-hsl-${mode}-${i}`}
          label={
            <>
              <span aria-hidden className="size-2.5 shrink-0 rounded-full" style={{ backgroundColor: HUE_SWATCHES[i] }} />
              {name}
            </>
          }
          value={adjust[mode][i] ?? 0}
          min={-100}
          max={100}
          step={1}
          onChange={(v) => onChange({ [mode]: adjust[mode].map((x, k) => (k === i ? v : x)) })}
        />
      ))}
    </Collapsible>
  );
}

const HUE_TRACK = 'linear-gradient(to right, hsl(0 85% 55%), hsl(60 85% 50%), hsl(120 70% 45%), hsl(180 70% 45%), hsl(240 75% 60%), hsl(300 75% 55%), hsl(360 85% 55%))';

/** วงล้อสี: มุม = สี (แดงอยู่บนสุด วนตามเข็มนาฬิกา) ระยะจากกลาง = ความเข้มของสี · ลูกศรซ้าย/ขวา = สี บน/ล่าง = ความเข้ม */
function ColorWheel({ wheel, onChange, label }: { wheel: Wheel; onChange: (w: Wheel) => void; label: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const pick = (event: React.PointerEvent) => {
    const r = ref.current!.getBoundingClientRect();
    const dx = event.clientX - (r.left + r.width / 2);
    const dy = event.clientY - (r.top + r.height / 2);
    const hue = Math.round(((Math.atan2(dx, -dy) * 180) / Math.PI + 360) % 360);
    const sat = Math.round(clamp(Math.hypot(dx, dy) / (r.width / 2), 0, 1) * 100);
    onChange({ ...wheel, hue, sat });
  };
  const rad = (wheel.hue * Math.PI) / 180;
  const key = (event: React.KeyboardEvent) => {
    const step = event.shiftKey ? 10 : 1;
    const next: Record<string, Partial<Wheel>> = {
      ArrowLeft: { hue: (wheel.hue - step + 360) % 360 },
      ArrowRight: { hue: (wheel.hue + step) % 360 },
      ArrowUp: { sat: clamp(wheel.sat + step, 0, 100) },
      ArrowDown: { sat: clamp(wheel.sat - step, 0, 100) },
    };
    if (next[event.key]) {
      event.preventDefault();
      onChange({ ...wheel, ...next[event.key] });
    }
  };
  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={0}
      aria-label={label}
      aria-valuetext={`สี ${wheel.hue}° ความเข้ม ${wheel.sat}`}
      aria-valuenow={wheel.sat}
      aria-valuemin={0}
      aria-valuemax={100}
      onKeyDown={key}
      onPointerDown={(e) => {
        dragging.current = true;
        try {
          ref.current?.setPointerCapture(e.pointerId);
        } catch {
          /* ตัวชี้สังเคราะห์ */
        }
        pick(e);
      }}
      onPointerMove={(e) => dragging.current && pick(e)}
      onPointerUp={() => (dragging.current = false)}
      onPointerCancel={() => (dragging.current = false)}
      onDoubleClick={() => onChange({ ...wheel, hue: 0, sat: 0 })}
      className="relative mx-auto aspect-square w-40 cursor-crosshair touch-none rounded-full shadow-[inset_0_0_0_1px_rgba(0,0,0,0.15)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--action)]"
      style={{
        background:
          'radial-gradient(circle closest-side, rgb(150 150 150) 0%, rgb(150 150 150 / 0) 100%), conic-gradient(from 0deg, hsl(0 90% 55%), hsl(60 90% 50%), hsl(120 75% 45%), hsl(180 75% 45%), hsl(240 80% 60%), hsl(300 80% 55%), hsl(360 90% 55%))',
      }}
      data-testid="editor-grading-wheel"
    >
      <span
        aria-hidden
        className="pointer-events-none absolute size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
        style={{ left: `${50 + Math.sin(rad) * (wheel.sat / 2)}%`, top: `${50 - Math.cos(rad) * (wheel.sat / 2)}%` }}
      />
    </div>
  );
}

type Zone = 'shadows' | 'midtones' | 'highlights' | 'global';

/** Color Grading แบบ Lightroom: เติมสีให้เงา ช่วงกลาง ส่วนสว่าง หรือทั้งภาพ */
export function ColorGradingPanel({ adjust, onChange }: { adjust: Adjust; onChange: (patch: Partial<Adjust>) => void }) {
  const [zone, setZone] = useState<Zone>('shadows');
  const g = adjust.grading;
  const wheel = g[zone];
  const setWheel = (w: Wheel) => onChange({ grading: { ...g, [zone]: w } });
  return (
    <Collapsible title="Color Grading" active={gradingIsActive(g)} onReset={() => onChange({ grading: GRADING_NEUTRAL })} testId="editor-grading">
      <Tabs<Zone>
        variant="pill"
        className="w-full [&>button]:flex-1"
        value={zone}
        onChange={setZone}
        label="ช่วงที่เติมสี"
        testId="editor-grading-zone"
        items={[
          { value: 'shadows', label: 'เงา' },
          { value: 'midtones', label: 'กลาง' },
          { value: 'highlights', label: 'สว่าง' },
          { value: 'global', label: 'ทั้งภาพ' },
        ]}
      />
      <ColorWheel wheel={wheel} onChange={setWheel} label={`วงล้อสีของ${{ shadows: 'เงา', midtones: 'ช่วงกลาง', highlights: 'ส่วนสว่าง', global: 'ทั้งภาพ' }[zone]}`} />
      <SliderRow id={`editor-grading-${zone}-hue`} label="สี" value={wheel.hue} min={0} max={359} step={1} track={HUE_TRACK} format={(v) => `${v}°`} onChange={(v) => setWheel({ ...wheel, hue: v })} />
      <SliderRow id={`editor-grading-${zone}-sat`} label="ความเข้มของสี" value={wheel.sat} min={0} max={100} step={1} format={(v) => `${v}`} onChange={(v) => setWheel({ ...wheel, sat: v })} />
      <SliderRow id={`editor-grading-${zone}-lum`} label="ความสว่าง" value={wheel.lum} min={-100} max={100} step={1} onChange={(v) => setWheel({ ...wheel, lum: v })} />
      <div className="border-t border-line pt-3">
        <SliderRow id="editor-grading-blending" label="การผสมช่วง (Blending)" value={g.blending} min={0} max={100} step={1} defaultValue={50} format={(v) => `${v}`} onChange={(v) => onChange({ grading: { ...g, blending: v } })} />
      </div>
      <SliderRow
        id="editor-grading-balance"
        label="สมดุล (Balance)"
        value={g.balance}
        min={-100}
        max={100}
        step={1}
        ends={['เน้นเงา', 'เน้นส่วนสว่าง']}
        onChange={(v) => onChange({ grading: { ...g, balance: v } })}
      />
    </Collapsible>
  );
}

/** ขอบภาพมืด/สว่าง (Vignette หลังตัด) */
/** ความคมแบบ Lightroom (DECISIONS ข้อ 36) — เห็นชัดในไฟล์ที่ส่งออกขนาดเต็ม ภาพตัวอย่างย่อจึงเห็นน้อย */
export function SharpenPanel({ adjust, onChange }: { adjust: Adjust; onChange: (patch: Partial<Adjust>) => void }) {
  const v = adjust.sharpen ?? SHARPEN_NEUTRAL;
  const set = (patch: Partial<typeof v>) => onChange({ sharpen: { ...v, ...patch } });
  return (
    <Collapsible title="ความคม (Sharpening)" active={hasSharpen(adjust)} onReset={() => onChange({ sharpen: SHARPEN_NEUTRAL })} testId="editor-sharpen">
      <SliderRow id="editor-sharpen-amount" label="ปริมาณ" value={v.amount} min={0} max={150} step={1} defaultValue={0} format={(x) => `${x}`} onChange={(x) => set({ amount: x })} />
      <SliderRow id="editor-sharpen-radius" label="รัศมี" value={v.radius} min={0.5} max={3} step={0.1} defaultValue={1} format={(x) => x.toFixed(1)} onChange={(x) => set({ radius: x })} />
      <SliderRow id="editor-sharpen-detail" label="รายละเอียด" value={v.detail} min={0} max={100} step={1} defaultValue={25} format={(x) => `${x}`} ends={['กันขอบเกิน', 'ทุกรายละเอียด']} onChange={(x) => set({ detail: x })} />
      <SliderRow id="editor-sharpen-masking" label="มาสก์ขอบ" value={v.masking} min={0} max={100} step={1} defaultValue={0} format={(x) => `${x}`} ends={['ทั้งภาพ', 'เฉพาะขอบ']} onChange={(x) => set({ masking: x })} />
      <p className="text-[12.5px] text-muted">
        {v.amount === 0 ? 'เลื่อน “ปริมาณ” ก่อน แถบอื่นจึงมีผล · ' : ''}เห็นชัดในไฟล์ที่ส่งออกขนาดเต็ม ภาพตัวอย่างย่ออยู่จึงเห็นน้อย (เหมือน Lightroom ที่ต้องขยาย 1:1)
      </p>
    </Collapsible>
  );
}

export function VignettePanel({ adjust, onChange }: { adjust: Adjust; onChange: (patch: Partial<Adjust>) => void }) {
  const v = adjust.vignette;
  const set = (patch: Partial<Adjust['vignette']>) => onChange({ vignette: { ...v, ...patch } });
  return (
    <Collapsible title="ขอบภาพ (Vignette)" active={hasVignette(adjust)} onReset={() => onChange({ vignette: VIGNETTE_NEUTRAL })} testId="editor-vignette">
      <SliderRow id="editor-vignette-amount" label="ปริมาณ" value={v.amount} min={-100} max={100} step={1} ends={['ขอบมืด', 'ขอบสว่าง']} onChange={(x) => set({ amount: x })} />
      <SliderRow id="editor-vignette-midpoint" label="จุดเริ่ม" value={v.midpoint} min={0} max={100} step={1} defaultValue={50} format={(x) => `${x}`} ends={['ใกล้กลางภาพ', 'ชิดขอบ']} onChange={(x) => set({ midpoint: x })} />
      <SliderRow id="editor-vignette-feather" label="ความนุ่ม" value={v.feather} min={0} max={100} step={1} defaultValue={50} format={(x) => `${x}`} onChange={(x) => set({ feather: x })} />
      <SliderRow id="editor-vignette-roundness" label="ความกลม" value={v.roundness} min={-100} max={100} step={1} ends={['เหลี่ยม', 'กลม']} onChange={(x) => set({ roundness: x })} />
      {v.amount === 0 && <p className="text-[12.5px] text-muted">เลื่อน “ปริมาณ” ก่อน แถบอื่นจึงมีผล</p>}
    </Collapsible>
  );
}
