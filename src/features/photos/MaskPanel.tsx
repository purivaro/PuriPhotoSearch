import { useEffect, useRef, useState } from 'react';
import { Download, Eye, Palette, PersonStanding, RotateCcw, ScanFace, Shirt, Sparkle, Sun, Trash2, TriangleAlert } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { InlineNotice, Spinner } from '@/components/ui/Feedback';
import { Switch } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { formatCount, newRequestId } from '@/lib/format';
import type { EditMask, FaceBox, PersonPart } from '@/lib/ipc/types';
import { NEUTRAL, isNeutral, type Adjust } from './adjust';
import { SliderRow } from './AdjustControls';
import { PART_LABELS, maskName } from './editSummary';
import { MAX_MASKS } from './masks';
import { COLOR, LIGHT, PRESENCE, SliderGroup } from './SliderGroup';
import { FillModelsInstall } from './FillModelsInstall';
import { missingFor, partsReady, type SubjectMapState } from './useSubjectMap';

// แถบของการปรับเฉพาะส่วน: เฉพาะค่าที่มีความหมายกับบริเวณเล็ก ๆ (จุดขาว/ดำ ความสด เส้นโค้ง ฯลฯ ปรับที่ทั้งภาพ)
const MASK_LIGHT = LIGHT.filter((s) => ['exposure', 'contrast', 'highlights', 'shadows'].includes(s.key));
const MASK_COLOR = COLOR.filter((s) => ['temperature', 'tint', 'saturation'].includes(s.key));

/** จุดเริ่มที่กดได้เลย แล้วค่อยปรับแถบต่อ — ค่าตั้งต้นของ "หน้าขาวใส" ของงานถ่ายรูปบุคคลทั่วไป */
const LOOKS: { label: string; adjust: Partial<Adjust> }[] = [
  { label: 'หน้าสว่างขึ้น', adjust: { exposure: 0.35, shadows: 15 } },
  { label: 'ผิวเนียน', adjust: { texture: -45, clarity: -10 } },
  { label: 'ผิวขาวใส', adjust: { exposure: 0.25, texture: -35, clarity: -10, saturation: -12, temperature: -6, tint: 4 } },
];

/** ส่วนของคนที่เลือกได้ (Select People ของ Lightroom — DECISIONS ข้อ 43) */
const PARTS: PersonPart[] = ['hair', 'faceSkin', 'bodySkin', 'clothes'];

interface MaskPanelProps {
  masks: EditMask[];
  faces: FaceBox[];
  selected: string | null;
  onSelect: (id: string | null) => void;
  onChange: (masks: EditMask[]) => void;
  /** แสดงบริเวณของ Mask นี้บนภาพ (null = ไม่แสดง) */
  onShowArea: (id: string | null) => void;
  /** ตัวแยกตัวคนและภาพน้ำหนักของรูปนี้ (Mask ตัวคน — DECISIONS ข้อ 40) */
  subject: SubjectMapState;
}

/** ปรับเฉพาะส่วน (DECISIONS ข้อ 28): เพิ่ม/เลือก/ลบบริเวณ และค่าแสงสีของบริเวณที่เลือก */
export function MaskPanel({ masks, faces, selected, onSelect, onChange, onShowArea, subject }: MaskPanelProps) {
  const current = masks.find((m) => m.id === selected) ?? null;
  const flash = useRef(0);
  useEffect(() => () => window.clearTimeout(flash.current), []);
  const subjectState = subject.status?.state;
  // กด "ตัวคน (AI)" ตอนที่ยังไม่มีตัวแยก: ถามดาวน์โหลด แล้วเพิ่ม Mask ให้เองเมื่อพร้อม
  const [wantSubject, setWantSubject] = useState(false);
  // กดส่วนของคนตอนที่ยังไม่มีตัวแยกส่วนของคน: ถามดาวน์โหลด แล้วเพิ่ม Mask ส่วนนั้นให้เองเมื่อพร้อม
  const [wantPart, setWantPart] = useState<PersonPart | null>(null);
  const partsOk = partsReady(subject.status);
  const partsUnsupported = subject.status?.state === 'unsupported' ? subject.status : subject.status?.parts.state === 'unsupported' ? subject.status.parts : null;
  // Mask ตัวคน/ส่วนของคนที่เพิ่งเพิ่ม: แสดงบริเวณเป็นสีแดงสักครู่หลังหาตัวคนเสร็จ
  const [flashSubject, setFlashSubject] = useState<string | null>(null);

  const update = (id: string, patch: Partial<EditMask>) => onChange(masks.map((m) => (m.id === id ? { ...m, ...patch } : m)));
  const setAdjust = (id: string, patch: Partial<Adjust>) => {
    const m = masks.find((x) => x.id === id);
    if (m) update(id, { adjust: { ...m.adjust, ...patch } });
  };

  const showBriefly = (id: string) => {
    onShowArea(id);
    window.clearTimeout(flash.current);
    flash.current = window.setTimeout(() => onShowArea(null), 1400);
  };

  const add = (area: EditMask['area']) => {
    const mask: EditMask = { id: newRequestId(), area, adjust: NEUTRAL, amount: 100, invert: false };
    onChange([...masks, mask]);
    onSelect(mask.id);
    return mask.id;
  };

  const addFaces = () => {
    // ให้เห็นทันทีว่าเลือกตรงไหน
    showBriefly(add({ kind: 'faces', boxes: faces.map((f) => [f.sourceBbox.x, f.sourceBbox.y, f.sourceBbox.w, f.sourceBbox.h]), skin: true }));
  };

  const addAi = (area: EditMask['area']) => {
    const id = add(area);
    // บริเวณขึ้นเมื่อหาตัวคนเสร็จ (ครั้งแรกของรูปราว 7 วินาที)
    onShowArea(id);
    setFlashSubject(id);
  };
  const addSubject = () => addAi({ kind: 'subject' });
  const addPart = (part: PersonPart) => addAi({ kind: 'people', parts: [part] });

  /** เพิ่ม/เอาส่วนออกจาก Mask ส่วนของคนที่เลือก — ต้องเหลืออย่างน้อยหนึ่งส่วน */
  const togglePart = (m: EditMask, part: PersonPart) => {
    if (m.area.kind !== 'people') return;
    const has = m.area.parts.includes(part);
    const parts = has ? m.area.parts.filter((p) => p !== part) : PARTS.filter((p) => p === part || (m.area.kind === 'people' && m.area.parts.includes(p)));
    if (parts.length > 0) update(m.id, { area: { kind: 'people', parts } });
  };

  useEffect(() => {
    if (flashSubject && subject.phase !== 'preparing' && subject.phase !== 'idle') {
      setFlashSubject(null);
      showBriefly(flashSubject);
    }
    // showBriefly ใช้แค่ ref กับ onShowArea
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flashSubject, subject.phase]);

  useEffect(() => {
    if (wantPart && partsOk) {
      setWantPart(null);
      if (masks.length < MAX_MASKS) addPart(wantPart);
    }
    // เพิ่มครั้งเดียวเมื่อตัวแยกส่วนของคนพร้อม
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wantPart, partsOk]);

  useEffect(() => {
    if (wantSubject && subjectState === 'ready') {
      setWantSubject(false);
      if (masks.length < MAX_MASKS) addSubject();
    }
    // เพิ่มครั้งเดียวเมื่อตัวแยกพร้อม
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wantSubject, subjectState]);

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
        <Button
          size="sm"
          icon={<PersonStanding className="size-4" aria-hidden />}
          onClick={() => (subjectState === 'ready' ? addSubject() : setWantSubject(true))}
          disabled={masks.length >= MAX_MASKS || subjectState === 'unsupported' || !subject.status}
          className="w-full"
          data-testid="mask-add-subject"
        >
          ตัวคน (AI)
        </Button>
        {subjectState === 'unsupported' ? (
          <p className="text-[12.5px] text-muted">{subject.status?.reason}</p>
        ) : (
          !wantSubject && <p className="text-[12.5px] text-muted">แอปเลือกตัวคนในรูปให้เอง แบบ Select Subject ของ Lightroom — ฉากหลังไม่โดน</p>
        )}
        {wantSubject && subjectState !== 'ready' && subjectState !== 'unsupported' && (
          <SubjectInstall subject={subject} onCancel={() => setWantSubject(false)} />
        )}
        <div className="space-y-1.5 pt-1" data-testid="mask-parts">
          <p className="flex items-center gap-1.5 text-[13px] font-semibold text-ink">
            <Shirt className="size-4 text-muted" aria-hidden /> ส่วนของคน (AI)
          </p>
          <div className="grid grid-cols-2 gap-1.5">
            {PARTS.map((part) => (
              <Button
                key={part}
                size="sm"
                variant={wantPart === part ? 'primary' : undefined}
                onClick={() => (partsOk ? addPart(part) : setWantPart(part))}
                disabled={masks.length >= MAX_MASKS || !!partsUnsupported || !subject.status}
                data-testid={`mask-add-part-${part}`}
              >
                {PART_LABELS[part]}
              </Button>
            ))}
          </div>
          {partsUnsupported ? (
            // ตัวแยกตัวคนใช้ไม่ได้: เหตุขึ้นใต้ปุ่ม "ตัวคน (AI)" แล้ว ไม่ซ้ำ
            subjectState !== 'unsupported' && (
              <p className="text-[12.5px] text-muted" data-testid="parts-unsupported">
                {partsUnsupported.reason}
              </p>
            )
          ) : (
            !wantPart && (
              <p className="text-[12.5px] text-muted">แอปแยกผม ผิว เสื้อผ้าของทุกคนในรูปให้เอง แบบ Select People ของ Lightroom · รวมหลายส่วนในบริเวณเดียวได้</p>
            )
          )}
          {wantPart && !partsOk && !partsUnsupported && (
            <FillModelsInstall
              subject={subject}
              kind="parts"
              onCancel={() => setWantPart(null)}
              title="ส่วนของคนต้องดาวน์โหลดก่อน (ครั้งเดียว)"
              after={`เสร็จแล้วแอปเพิ่ม Mask ${PART_LABELS[wantPart]}ให้เอง`}
              testId="parts"
            />
          )}
        </div>
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
          {current.area.kind === 'people' && (
            <div className="space-y-1.5">
              <p className="text-[13px] font-semibold text-ink">ส่วนที่รวมในบริเวณนี้</p>
              <div className="flex flex-wrap gap-1.5" role="group" aria-label="ส่วนของคนที่รวม">
                {PARTS.map((part) => {
                  const on = current.area.kind === 'people' && current.area.parts.includes(part);
                  const last = on && current.area.kind === 'people' && current.area.parts.length === 1;
                  return (
                    <button
                      key={part}
                      type="button"
                      onClick={() => togglePart(current, part)}
                      disabled={last}
                      aria-pressed={on}
                      title={last ? 'ต้องมีอย่างน้อยหนึ่งส่วน' : undefined}
                      className={cn(
                        'h-8 rounded-full border px-3 text-[12.5px] font-semibold transition-colors disabled:cursor-not-allowed',
                        on ? 'border-action bg-action text-white' : 'border-line-strong bg-surface text-ink hover:border-action hover:text-action',
                      )}
                      data-testid="mask-part"
                    >
                      {PART_LABELS[part]}
                    </button>
                  );
                })}
              </div>
              <p className="text-[12px] text-muted">ทุกคนในรูป · ตา ปาก ฟัน คิ้ว ยังแยกไม่ได้ในรุ่นนี้</p>
            </div>
          )}
          {current.area.kind === 'faces' && (
            <Switch
              checked={current.area.skin}
              onCheckedChange={(skin) => update(current.id, { area: { kind: 'faces', boxes: current.area.kind === 'faces' ? current.area.boxes : [], skin } })}
              label="เฉพาะผิว"
              description="จีวร ผม เสื้อผ้า และฉากหลังในบริเวณใบหน้าไม่โดน"
              testId="mask-skin"
            />
          )}
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
          {current.area.kind === 'faces' && current.area.skin && current.area.boxes.length > 0 && (
            <InlineNotice tone="info">ผิวที่อยู่นอกบริเวณใบหน้า (แขน มือ) ไม่รวม — ใช้ "ผิวตัว" ในส่วนของคน (AI) แทน</InlineNotice>
          )}
        </div>
      )}
    </div>
  );
}

/** ถามดาวน์โหลดตัวแยกตัวคน (ครั้งเดียว) — ระหว่างดาวน์โหลดแต่งส่วนอื่นได้ เสร็จแล้วเพิ่ม Mask ให้เอง */
function SubjectInstall({ subject, onCancel }: { subject: SubjectMapState; onCancel: () => void }) {
  const status = subject.status;
  if (!status) return null;
  if (subject.installing) {
    return (
      <div className="flex items-start gap-2 rounded-xl border border-line bg-sunken/60 p-3 text-[13px] text-ink" data-testid="subject-installing">
        <Spinner className="mt-0.5 size-4 shrink-0" />
        <span>กำลังดาวน์โหลดตัวแยกตัวคน ({formatCount(status.downloadSizeMb)} MB) — แต่งส่วนอื่นได้ระหว่างรอ เสร็จแล้วแอปเพิ่ม Mask ตัวคนให้เอง</span>
      </div>
    );
  }
  return (
    <div className="space-y-2 rounded-xl border border-line bg-sunken/60 p-3" data-testid="subject-install">
      <p className="text-sm font-semibold text-ink">ดาวน์โหลดตัวแยกตัวคน</p>
      <p className="text-[13px] text-muted">{status.reason}</p>
      <p className="text-[12px] text-muted">
        {status.modelName} · {status.licenseNote} · ที่มา {status.downloadSource}
      </p>
      <div className="flex gap-1.5">
        <Button size="sm" variant="primary" icon={<Download className="size-4" aria-hidden />} onClick={subject.install} data-testid="subject-install-start">
          ดาวน์โหลด ({formatCount(status.downloadSizeMb)} MB)
        </Button>
        <Button size="sm" onClick={onCancel}>
          ไว้ทีหลัง
        </Button>
      </div>
    </div>
  );
}

/** สถานะของ Mask ตัวคน / ตัวผอมของรูป: กำลังหาตัวคน / ไม่พบ / ต้องดาวน์โหลด / ผิดพลาด */
export function SubjectProgress({ subject }: { subject: SubjectMapState }) {
  const status = subject.status;
  switch (subject.phase) {
    case 'preparing':
      return (
        <InlineNotice tone="info" icon={<Spinner className="size-4" />}>
          <span data-testid="subject-preparing">
            {subject.body ? 'กำลังหาตัวคนและเตรียมฉากหลังของรูปนี้…' : subject.parts ? 'กำลังหาตัวคนและแยกส่วนของคนในรูปนี้…' : 'กำลังหาตัวคนในรูปนี้…'}{' '}
            ครั้งแรกของแต่ละรูปราว 10 วินาที
          </span>
        </InlineNotice>
      );
    case 'ready':
      return subject.found === false ? (
        <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
          <span data-testid="subject-none">ไม่พบตัวคนในรูปนี้ {subject.body ? 'ตัวผอม' : subject.parts ? 'Mask ส่วนของคน' : 'Mask ตัวคน'}จึงยังไม่มีผล</span>
        </InlineNotice>
      ) : null;
    case 'failed':
      return (
        <InlineNotice
          tone="danger"
          icon={<TriangleAlert aria-hidden />}
          action={
            <Button size="sm" icon={<RotateCcw className="size-4" aria-hidden />} onClick={subject.retry}>
              ลองใหม่
            </Button>
          }
        >
          <span data-testid="subject-failed">{subject.error}</span>
        </InlineNotice>
      );
    case 'needs_model': {
      const unsupported =
        status?.state === 'unsupported'
          ? status
          : subject.body && status?.fill.state === 'unsupported'
            ? status.fill
            : subject.parts && status?.parts.state === 'unsupported'
              ? status.parts
              : null;
      const size = missingFor(status, subject).reduce((sum, m) => sum + m.downloadSizeMb, 0);
      const what = ['ตัวแยกตัวคน', ...(subject.body ? ['ตัวเติมฉากหลัง'] : []), ...(subject.parts ? ['ตัวแยกส่วนของคน'] : [])].join('และ');
      return unsupported ? (
        <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
          {unsupported.reason}
        </InlineNotice>
      ) : subject.installing ? (
        <InlineNotice tone="info" icon={<Spinner className="size-4" />}>
          กำลังดาวน์โหลด{what} — เห็นผลเมื่อดาวน์โหลดเสร็จ
        </InlineNotice>
      ) : (
        <InlineNotice
          tone="warn"
          icon={<TriangleAlert aria-hidden />}
          action={
            <Button size="sm" icon={<Download className="size-4" aria-hidden />} onClick={subject.install} data-testid="subject-needs-install">
              ดาวน์โหลด ({formatCount(size)} MB)
            </Button>
          }
        >
          <span data-testid="subject-needs-model">
            {subject.body ? 'รูปนี้ใช้ตัวผอม' : subject.parts ? 'รูปนี้มี Mask ส่วนของคน' : 'รูปนี้มี Mask ตัวคน'} ต้องดาวน์โหลด{what}ก่อนจึงจะเห็นผล (ครั้งเดียว)
          </span>
        </InlineNotice>
      );
    }
    default:
      return null;
  }
}
