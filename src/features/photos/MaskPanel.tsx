import { useEffect, useRef } from 'react';
import { Eye, Palette, ScanFace, Sparkle, Sun, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { InlineNotice } from '@/components/ui/Feedback';
import { Switch } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { formatCount, newRequestId } from '@/lib/format';
import type { EditMask, FaceBox } from '@/lib/ipc/types';
import { NEUTRAL, isNeutral, type Adjust } from './adjust';
import { SliderRow } from './AdjustControls';
import { maskName } from './editSummary';
import { MAX_MASKS } from './masks';
import { COLOR, LIGHT, PRESENCE, SliderGroup } from './SliderGroup';

// แถบของการปรับเฉพาะส่วน: เฉพาะค่าที่มีความหมายกับบริเวณเล็ก ๆ (จุดขาว/ดำ ความสด เส้นโค้ง ฯลฯ ปรับที่ทั้งภาพ)
const MASK_LIGHT = LIGHT.filter((s) => ['exposure', 'contrast', 'highlights', 'shadows'].includes(s.key));
const MASK_COLOR = COLOR.filter((s) => ['temperature', 'tint', 'saturation'].includes(s.key));

/** จุดเริ่มที่กดได้เลย แล้วค่อยปรับแถบต่อ — ค่าตั้งต้นของ "หน้าขาวใส" ของงานถ่ายรูปบุคคลทั่วไป */
const LOOKS: { label: string; adjust: Partial<Adjust> }[] = [
  { label: 'หน้าสว่างขึ้น', adjust: { exposure: 0.35, shadows: 15 } },
  { label: 'ผิวเนียน', adjust: { texture: -45, clarity: -10 } },
  { label: 'ผิวขาวใส', adjust: { exposure: 0.25, texture: -35, clarity: -10, saturation: -12, temperature: -6, tint: 4 } },
];

interface MaskPanelProps {
  masks: EditMask[];
  faces: FaceBox[];
  selected: string | null;
  onSelect: (id: string | null) => void;
  onChange: (masks: EditMask[]) => void;
  /** แสดงบริเวณของ Mask นี้บนภาพ (null = ไม่แสดง) */
  onShowArea: (id: string | null) => void;
}

/** ปรับเฉพาะส่วน (DECISIONS ข้อ 28): เพิ่ม/เลือก/ลบบริเวณ และค่าแสงสีของบริเวณที่เลือก */
export function MaskPanel({ masks, faces, selected, onSelect, onChange, onShowArea }: MaskPanelProps) {
  const current = masks.find((m) => m.id === selected) ?? null;
  const flash = useRef(0);
  useEffect(() => () => window.clearTimeout(flash.current), []);

  const update = (id: string, patch: Partial<EditMask>) => onChange(masks.map((m) => (m.id === id ? { ...m, ...patch } : m)));
  const setAdjust = (id: string, patch: Partial<Adjust>) => {
    const m = masks.find((x) => x.id === id);
    if (m) update(id, { adjust: { ...m.adjust, ...patch } });
  };

  const addFaces = () => {
    const mask: EditMask = {
      id: newRequestId(),
      area: { kind: 'faces', boxes: faces.map((f) => [f.sourceBbox.x, f.sourceBbox.y, f.sourceBbox.w, f.sourceBbox.h]), skin: true },
      adjust: NEUTRAL,
      amount: 100,
      invert: false,
    };
    onChange([...masks, mask]);
    onSelect(mask.id);
    // ให้เห็นทันทีว่าเลือกตรงไหน
    onShowArea(mask.id);
    window.clearTimeout(flash.current);
    flash.current = window.setTimeout(() => onShowArea(null), 1400);
  };

  return (
    <div className="space-y-4" data-testid="mask-panel">
      <section className="space-y-2">
        <h3 className="flex items-center gap-1.5 text-sm font-semibold text-ink">
          <ScanFace className="size-4 text-muted" aria-hidden /> ปรับเฉพาะส่วน
        </h3>
        {masks.length === 0 && (
          <p className="text-[13px] text-muted">ปรับแสงสีเฉพาะบางส่วนของภาพ เช่น ทำหน้าให้สว่างและผิวเนียน โดยจีวร เสื้อผ้า และฉากหลังไม่เปลี่ยน</p>
        )}
        <Button
          size="sm"
          icon={<ScanFace className="size-4" aria-hidden />}
          onClick={addFaces}
          disabled={faces.length === 0 || masks.length >= MAX_MASKS}
          className="w-full"
          data-testid="mask-add-faces"
        >
          ใบหน้าและผิว{faces.length > 0 ? ` (${formatCount(faces.length)} คน)` : ''}
        </Button>
        {faces.length === 0 && (
          <p className="text-[12.5px] text-muted">ยังไม่มีใบหน้าในรูปนี้ — ให้แอปวิเคราะห์ใบหน้า หรือเพิ่มกรอบใบหน้าเองในตัวดูภาพก่อน</p>
        )}
      </section>

      {masks.length > 0 && (
        <ul className="space-y-1.5" aria-label="บริเวณที่ปรับ">
          {masks.map((m) => (
            <li key={m.id} className="flex items-center gap-1">
              <button
                type="button"
                onClick={() => onSelect(m.id === selected ? null : m.id)}
                aria-pressed={m.id === selected}
                className={cn(
                  'min-w-0 flex-1 rounded-lg border px-2.5 py-2 text-left text-[13px] transition-colors',
                  m.id === selected ? 'border-action bg-brand-soft text-ink' : 'border-line bg-surface text-ink hover:bg-sunken',
                )}
                data-testid="mask-item"
              >
                <span className="block truncate font-semibold">{maskName(m)}</span>
                <span className="block truncate text-[12px] text-muted">{isNeutral(m.adjust) ? 'ยังไม่ได้ปรับ' : `ความแรง ${formatCount(m.amount)}%`}</span>
              </button>
              <button
                type="button"
                onClick={() => {
                  onChange(masks.filter((x) => x.id !== m.id));
                  if (selected === m.id) onSelect(null);
                }}
                className="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted hover:bg-sunken hover:text-danger"
                aria-label={`ลบ ${maskName(m)}`}
                data-testid="mask-delete"
              >
                <Trash2 className="size-4" aria-hidden />
              </button>
            </li>
          ))}
        </ul>
      )}

      {current && (
        <div className="space-y-4 border-t border-line pt-3" data-testid="mask-editor">
          <div className="flex flex-wrap gap-1.5" role="group" aria-label="จุดเริ่มสำเร็จรูป">
            {LOOKS.map((look) => (
              <button
                key={look.label}
                type="button"
                onClick={() => update(current.id, { adjust: { ...NEUTRAL, ...look.adjust } })}
                className="h-8 rounded-full border border-line-strong bg-surface px-3 text-[12.5px] font-semibold text-ink hover:border-action hover:text-action"
                data-testid="mask-look"
              >
                {look.label}
              </button>
            ))}
          </div>
          <Switch
            checked={current.area.skin}
            onCheckedChange={(skin) => update(current.id, { area: { ...current.area, skin } })}
            label="เฉพาะผิว"
            description="จีวร ผม เสื้อผ้า และฉากหลังในบริเวณใบหน้าไม่โดน"
            testId="mask-skin"
          />
          <Switch
            checked={current.invert}
            onCheckedChange={(invert) => update(current.id, { invert })}
            label="กลับด้าน"
            description="ปรับทุกส่วนของภาพ ยกเว้นบริเวณนี้"
            testId="mask-invert"
          />
          <SliderRow
            id="mask-amount"
            label="ความแรง"
            value={current.amount}
            min={0}
            max={100}
            step={1}
            defaultValue={100}
            format={(v) => `${formatCount(v)}%`}
            onChange={(amount) => update(current.id, { amount })}
          />
          <button
            type="button"
            onPointerDown={() => onShowArea(current.id)}
            onPointerUp={() => onShowArea(null)}
            onPointerLeave={() => onShowArea(null)}
            onKeyDown={(e) => e.key === ' ' && onShowArea(current.id)}
            onKeyUp={() => onShowArea(null)}
            className="flex h-9 w-full items-center justify-center gap-1.5 rounded-lg border border-line-strong text-[13px] font-semibold text-ink hover:bg-sunken"
            data-testid="mask-show-area"
          >
            <Eye className="size-4" aria-hidden /> กดค้างเพื่อดูบริเวณ
          </button>
          <SliderGroup title="แสง" icon={<Sun className="size-4 text-muted" aria-hidden />} specs={MASK_LIGHT} adjust={current.adjust} onChange={(p) => setAdjust(current.id, p)} idPrefix="mask" />
          <SliderGroup title="รายละเอียด" icon={<Sparkle className="size-4 text-muted" aria-hidden />} specs={PRESENCE} adjust={current.adjust} onChange={(p) => setAdjust(current.id, p)} idPrefix="mask" />
          <SliderGroup title="สี" icon={<Palette className="size-4 text-muted" aria-hidden />} specs={MASK_COLOR} adjust={current.adjust} onChange={(p) => setAdjust(current.id, p)} idPrefix="mask" />
          {current.area.skin && current.area.boxes.length > 0 && (
            <InlineNotice tone="info">ผิวที่อยู่นอกบริเวณใบหน้า (แขน มือ) ยังไม่รวม — รุ่นนี้ปรับเฉพาะใบหน้าที่แอปพบ</InlineNotice>
          )}
        </div>
      )}
    </div>
  );
}
