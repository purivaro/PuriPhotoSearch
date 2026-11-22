import { useRef, useState } from 'react';
import {
  linearFromDrag,
  linearShape,
  moveLinear,
  moveRadial,
  radialFromDrag,
  radialShape,
  type GradientArea,
  type LinearArea,
  type LinearHandle,
  type Mapper,
  type Px,
  type RadialArea,
  type RadialHandle,
} from './gradients';

/** วาด Mask ใหม่: ไล่ระดับ หรือวงกลม */
export type GradientTool = 'linear' | 'radial';

interface GradientOverlayProps {
  mapper: Mapper;
  boxW: number;
  boxH: number;
  /** Mask ไล่ระดับ/วงกลมที่เลือกอยู่ (แสดงเส้นและจุดจับ) */
  area: GradientArea | null;
  onChange: (area: GradientArea) => void;
  /** กำลังวาดอันใหม่ */
  tool: GradientTool | null;
  onCreate: (area: GradientArea) => void;
}

const toPoints = (pts: Px[]) => pts.map((p) => `${p.x},${p.y}`).join(' ');

/** เส้นสองชั้น (ขาวทับดำจาง) ให้เห็นได้ทั้งบนฟ้าสว่างและพื้นมืด */
function Line({ a, b, dashed, strong }: { a: Px; b: Px; dashed?: boolean; strong?: boolean }) {
  const dash = dashed ? '6 5' : undefined;
  return (
    <>
      <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke="black" strokeOpacity={0.45} strokeWidth={strong ? 3.5 : 3} strokeDasharray={dash} />
      <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke="white" strokeWidth={strong ? 2 : 1.5} strokeDasharray={dash} />
    </>
  );
}

function Ring({ pts, dashed }: { pts: Px[]; dashed?: boolean }) {
  const dash = dashed ? '6 5' : undefined;
  return (
    <>
      <polygon points={toPoints(pts)} fill="none" stroke="black" strokeOpacity={0.45} strokeWidth={3} strokeDasharray={dash} />
      <polygon points={toPoints(pts)} fill="none" stroke="white" strokeWidth={1.5} strokeDasharray={dash} />
    </>
  );
}

/**
 * เส้นและจุดจับของ Mask ไล่ระดับ/วงกลมบนภาพ (DECISIONS ข้อ 45) แบบ Lightroom:
 * ไล่ระดับ = เส้นเต็มที่ เส้นกลาง เส้นจางหมด · วงกลม = ขอบนอก ขอบในที่เริ่มนุ่ม จุดจับกลาง/แกน · ลากบนภาพเพื่อสร้างอันใหม่
 */
export function GradientOverlay({ mapper, boxW, boxH, area, onChange, tool, onCreate }: GradientOverlayProps) {
  const root = useRef<HTMLDivElement>(null);
  // การลากที่กำลังทำ: สร้างอันใหม่ (จุดเริ่ม–จุดปัจจุบัน) หรือจับจุดจับของอันที่เลือก
  const [draft, setDraft] = useState<{ from: Px; to: Px } | null>(null);
  const grab = useRef<{ handle: LinearHandle | RadialHandle; from: Px; area: GradientArea } | null>(null);

  const local = (e: React.PointerEvent): Px => {
    const r = root.current?.getBoundingClientRect();
    return r ? { x: e.clientX - r.left, y: e.clientY - r.top } : { x: 0, y: 0 };
  };

  const preview: GradientArea | null =
    tool && draft ? (tool === 'linear' ? linearFromDrag(mapper, draft.from, draft.to) : radialFromDrag(mapper, draft.from, draft.to, boxW, boxH)) : null;
  const shown = preview ?? (tool ? null : area);

  const startHandle = (handle: LinearHandle | RadialHandle) => (e: React.PointerEvent) => {
    if (!area) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    grab.current = { handle, from: local(e), area };
  };
  const dragHandle = (e: React.PointerEvent) => {
    const g = grab.current;
    if (!g) return;
    const p = local(e);
    if (g.area.kind === 'linear') {
      onChange(moveLinear(mapper, g.area, g.handle as LinearHandle, p, { from: g.from, area: g.area }));
    } else {
      onChange(moveRadial(mapper, g.area, g.handle as RadialHandle, p, { from: g.from, center: g.area.center }));
    }
  };
  const endHandle = () => {
    grab.current = null;
  };

  const handle = (key: string, p: Px, which: LinearHandle | RadialHandle, label: string, big = false) => (
    <button
      key={key}
      type="button"
      aria-label={label}
      title={label}
      onPointerDown={startHandle(which)}
      onPointerMove={dragHandle}
      onPointerUp={endHandle}
      onPointerCancel={endHandle}
      className={
        'absolute z-30 -translate-x-1/2 -translate-y-1/2 cursor-grab touch-none rounded-full border-2 border-white bg-action shadow-[0_0_0_1.5px_rgba(0,0,0,0.45)] active:cursor-grabbing ' +
        (big ? 'size-4' : 'size-3.5')
      }
      style={{ left: p.x, top: p.y }}
      data-testid={`gradient-handle-${which}`}
    />
  );

  return (
    <div ref={root} className="pointer-events-none absolute inset-0 overflow-hidden" data-testid="gradient-overlay">
      {shown && (
        <svg aria-hidden className="absolute inset-0 size-full overflow-visible">
          {shown.kind === 'linear' ? <LinearLines area={shown} mapper={mapper} boxW={boxW} boxH={boxH} /> : <RadialLines area={shown} mapper={mapper} />}
        </svg>
      )}
      {!tool && area && (
        <div className="pointer-events-auto">
          {area.kind === 'linear'
            ? (() => {
                const s = linearShape(mapper, area, boxW, boxH);
                return [
                  handle('full', s.full, 'full', 'จุดเต็มที่ — ลากเพื่อย้าย'),
                  handle('mid', s.mid, 'mid', 'กลาง — ลากเพื่อย้ายทั้งหมด', true),
                  handle('zero', s.zero, 'zero', 'จุดจางหมด — ลากเพื่อย้าย'),
                ];
              })()
            : (() => {
                const s = radialShape(mapper, area);
                return [
                  handle('center', s.center, 'center', 'กลางวง — ลากเพื่อย้าย', true),
                  handle('x', s.xHandle, 'x', 'ขอบวง — ลากเพื่อปรับขนาดและหมุน'),
                  handle('y', s.yHandle, 'y', 'ขอบวงอีกแนว — ลากเพื่อปรับขนาด'),
                ];
              })()}
        </div>
      )}
      {tool && (
        <div
          className="pointer-events-auto absolute inset-0 z-20 cursor-crosshair touch-none"
          data-testid="gradient-surface"
          onPointerDown={(e) => {
            e.preventDefault();
            e.stopPropagation();
            (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
            const p = local(e);
            setDraft({ from: p, to: p });
          }}
          onPointerMove={(e) => draft && setDraft({ ...draft, to: local(e) })}
          onPointerUp={(e) => {
            if (!draft) return;
            const to = local(e);
            const made = tool === 'linear' ? linearFromDrag(mapper, draft.from, to) : radialFromDrag(mapper, draft.from, to, boxW, boxH);
            setDraft(null);
            if (made) onCreate(made);
          }}
          onPointerCancel={() => setDraft(null)}
        />
      )}
    </div>
  );
}

function LinearLines({ area, mapper, boxW, boxH }: { area: LinearArea; mapper: Mapper; boxW: number; boxH: number }) {
  const s = linearShape(mapper, area, boxW, boxH);
  const [full, mid, zero] = s.lines;
  return (
    <>
      <Line a={full[0]} b={full[1]} strong />
      <Line a={mid[0]} b={mid[1]} dashed />
      <Line a={zero[0]} b={zero[1]} />
      <Line a={s.full} b={s.zero} dashed />
    </>
  );
}

function RadialLines({ area, mapper }: { area: RadialArea; mapper: Mapper }) {
  const s = radialShape(mapper, area);
  return (
    <>
      <Ring pts={s.outer} />
      {s.inner && <Ring pts={s.inner} dashed />}
    </>
  );
}
