import { useMemo, useState, type ReactNode } from 'react';
import { ChevronDown } from 'lucide-react';
import { Tabs } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { HUE_NAMES, HUE_SWATCHES, NEUTRAL, curves, hasCurves, hasHsl, type Adjust } from './adjust';

const signed = (v: number) => (v > 0 ? `+${v}` : `${v}`);

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
  onChange: (value: number) => void;
}

/** แถบเลื่อนหนึ่งค่า — ดับเบิลคลิกชื่อหรือแถบเพื่อคืนเป็น 0 แบบ Lightroom */
export function SliderRow({ id, label, value, min, max, step, ends, format, onChange }: SliderRowProps) {
  return (
    <div>
      <div className="flex items-baseline justify-between gap-2">
        <label htmlFor={id} className="flex min-w-0 cursor-default items-center gap-1.5 text-[13.5px] text-ink select-none" onDoubleClick={() => onChange(0)} title="ดับเบิลคลิกเพื่อคืนเป็น 0">
          {label}
        </label>
        <span className={cn('shrink-0 text-[13px] tabular-nums', value === 0 ? 'text-muted' : 'font-semibold text-action')}>{format ? format(value) : signed(value)}</span>
      </div>
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        onDoubleClick={() => onChange(0)}
        className="h-1.5 w-full cursor-pointer accent-[var(--action)]"
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

/** ชื่อ Parametric curve เรียงจากสว่างลงมืดแบบ Lightroom (ค่าเก็บเรียงมืด → สว่าง) */
const REGIONS: { index: number; label: string }[] = [
  { index: 3, label: 'สว่างจัด' },
  { index: 2, label: 'สว่าง' },
  { index: 1, label: 'มืด' },
  { index: 0, label: 'มืดจัด' },
];

const CURVE_KEYS = ['parametric', 'parametricSplit', 'curve', 'curveRed', 'curveGreen', 'curveBlue'] as const;

/** กราฟเส้นโค้งที่ได้ (รวม Parametric + เส้นโค้งแบบจุด) เส้นเดียวถ้าทุกช่องสีเท่ากัน */
function CurveGraph({ adjust }: { adjust: Adjust }) {
  const paths = useMemo(() => {
    const only: Adjust = { ...NEUTRAL, parametric: adjust.parametric, parametricSplit: adjust.parametricSplit, curve: adjust.curve, curveRed: adjust.curveRed, curveGreen: adjust.curveGreen, curveBlue: adjust.curveBlue };
    const tables = curves(only);
    const toPath = (t: Float64Array) => Array.from({ length: 52 }, (_, i) => Math.min(255, i * 5)).map((x, i) => `${i === 0 ? 'M' : 'L'}${((x / 255) * 100).toFixed(2)},${(100 - (t[x] ?? 0) * 100).toFixed(2)}`).join(' ');
    const same = tables[0].every((v, i) => Math.abs(v - (tables[1][i] ?? 0)) < 1e-6 && Math.abs(v - (tables[2][i] ?? 0)) < 1e-6);
    return same ? [{ d: toPath(tables[0]), color: 'var(--ink)' }] : tables.map((t, c) => ({ d: toPath(t), color: ['#e5484d', '#3fa34d', '#3b6fd8'][c]! }));
  }, [adjust.parametric, adjust.parametricSplit, adjust.curve, adjust.curveRed, adjust.curveGreen, adjust.curveBlue]);
  return (
    <svg viewBox="0 0 100 100" className="aspect-square w-full rounded-lg border border-line bg-sunken" role="img" aria-label="กราฟเส้นโค้งโทน" data-testid="editor-curve-graph">
      {[25, 50, 75].map((v) => (
        <g key={v} stroke="var(--line)" strokeWidth="0.5">
          <line x1={v} y1="0" x2={v} y2="100" />
          <line x1="0" y1={v} x2="100" y2={v} />
        </g>
      ))}
      <line x1="0" y1="100" x2="100" y2="0" stroke="var(--line-strong)" strokeWidth="0.5" strokeDasharray="2 2" />
      {paths.map((p, i) => (
        <path key={i} d={p.d} fill="none" stroke={p.color} strokeWidth="1.4" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
      ))}
    </svg>
  );
}

export function ToneCurvePanel({ adjust, onChange }: { adjust: Adjust; onChange: (patch: Partial<Adjust>) => void }) {
  const points = [adjust.curve, adjust.curveRed, adjust.curveGreen, adjust.curveBlue].some((c) => c.length > 0);
  const reset = () => onChange(Object.fromEntries(CURVE_KEYS.map((k) => [k, NEUTRAL[k]])) as Partial<Adjust>);
  return (
    <Collapsible title="เส้นโค้งโทน" active={hasCurves(adjust)} onReset={reset} testId="editor-curve">
      <CurveGraph adjust={adjust} />
      {points && <p className="text-[12.5px] text-muted">มีเส้นโค้งแบบจุดจาก preset (แก้จุดในแอปนี้ไม่ได้ ปรับต่อด้วยแถบด้านล่างได้)</p>}
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
