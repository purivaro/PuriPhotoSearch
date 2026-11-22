import type { EditSettings, MaskArea, UnitRect } from '@/lib/ipc/types';
import { fromFrame, toFrame } from './editGeometry';

/**
 * Mask ไล่ระดับ / วงกลม (DECISIONS ข้อ 45) ฝั่ง UI: แปลงระหว่างจุดบนจอ (พิกเซลในกรอบภาพที่แสดง) กับภาพเดิม และจุดจับของแต่ละแบบ
 * — น้ำหนักคิดใน Rust (`crates/core/src/masks.rs`) · ตำแหน่งเก็บเป็นสัดส่วนบนภาพเดิม (ก่อนตัด/หมุน) ภาพหมุน/เอียงแล้ว Mask อยู่ที่เดิมบนภาพ
 */
export type LinearArea = Extract<MaskArea, { kind: 'linear' }>;
export type RadialArea = Extract<MaskArea, { kind: 'radial' }>;
export type GradientArea = LinearArea | RadialArea;
export type Px = { x: number; y: number };

/** ขอบนุ่มตั้งต้นของวงกลม (เท่ากับ Lightroom และ masks::RADIAL_FEATHER ใน Rust) */
export const RADIAL_FEATHER = 50;
/** ลากสั้นกว่านี้ (พิกเซลบนจอ) = กดเฉย ๆ: ไล่ระดับไม่สร้าง · วงกลมได้ขนาดตั้งต้น */
export const MIN_GRADIENT_DRAG = 8;

export const isGradient = (area: MaskArea): area is GradientArea => area.kind === 'linear' || area.kind === 'radial';

/** ตัวแปลงจุดของภาพที่แสดงอยู่ (กว้าง `boxW` สูง `boxH` พิกเซล แสดงส่วน `view` ของกรอบหลังปรับเอียง) */
export interface Mapper {
  /** ขนาดภาพเดิม (พิกเซล หลังหมุนตาม EXIF ก่อนหมุนของการแต่ง) */
  w: number;
  h: number;
  toScreen: (p: [number, number]) => Px;
  toSource: (p: Px) => [number, number];
}

export function mapper(width: number, height: number, s: EditSettings, view: UnitRect, boxW: number, boxH: number): Mapper {
  return {
    w: Math.max(width, 1),
    h: Math.max(height, 1),
    toScreen: ([u, v]) => {
      const f = toFrame(width, height, s, u, v);
      return { x: ((f.x - view.x) / view.w) * boxW, y: ((f.y - view.y) / view.h) * boxH };
    },
    toSource: ({ x, y }) => {
      const p = fromFrame(width, height, s, view.x + (x / boxW) * view.w, view.y + (y / boxH) * view.h);
      return [p.x, p.y];
    },
  };
}

const round = (v: number) => Math.round(v * 1e6) / 1e6;
const pt = (p: [number, number]): [number, number] => [round(p[0]), round(p[1])];

/** ไล่ระดับจากการลาก: จุดที่เริ่มลาก = เต็มที่ · จุดที่ปล่อย = จางหมด · ลากสั้นเกิน = null */
export function linearFromDrag(m: Mapper, from: Px, to: Px): LinearArea | null {
  if (Math.hypot(to.x - from.x, to.y - from.y) < MIN_GRADIENT_DRAG) return null;
  return { kind: 'linear', full: pt(m.toSource(from)), zero: pt(m.toSource(to)) };
}

/** เวกเตอร์บนภาพเดิมเป็นพิกเซล (สัดส่วนจริง) */
const sourceVector = (m: Mapper, a: [number, number], b: [number, number]) => ({ x: (b[0] - a[0]) * m.w, y: (b[1] - a[1]) * m.h });

/** วงกลมจากการลาก: จุดที่เริ่มลาก = ศูนย์กลาง · ระยะที่ลาก = รัศมีแนวนอน/ตั้งบนจอ (ตั้งตรงบนจอ) · กดเฉย ๆ = ขนาดตั้งต้น */
export function radialFromDrag(m: Mapper, from: Px, to: Px, boxW: number, boxH: number): RadialArea {
  const short = Math.hypot(to.x - from.x, to.y - from.y) < MIN_GRADIENT_DRAG;
  const sx = short ? Math.min(boxW, boxH) * 0.18 : Math.max(Math.abs(to.x - from.x), MIN_GRADIENT_DRAG);
  const sy = short ? Math.min(boxW, boxH) * 0.18 : Math.max(Math.abs(to.y - from.y), MIN_GRADIENT_DRAG);
  const c = m.toSource(from);
  const a = sourceVector(m, c, m.toSource({ x: from.x + sx, y: from.y }));
  const b = sourceVector(m, c, m.toSource({ x: from.x, y: from.y + sy }));
  return {
    kind: 'radial',
    center: pt(c),
    radius: [round(Math.hypot(a.x, a.y) / m.w), round(Math.hypot(b.x, b.y) / m.h)],
    angle: round((Math.atan2(a.y, a.x) * 180) / Math.PI),
    feather: RADIAL_FEATHER,
  };
}

/** จุดบนขอบวงรีที่ระยะ `k` ของรัศมี (k = 1 ขอบนอก) ที่มุม `t` (เรเดียน) บนภาพเดิม */
function radialPoint(m: Mapper, r: RadialArea, k: number, t: number): [number, number] {
  const a = (r.angle * Math.PI) / 180;
  const [rx, ry] = [r.radius[0] * m.w * k, r.radius[1] * m.h * k];
  const [x, y] = [Math.cos(t) * rx, Math.sin(t) * ry];
  return [r.center[0] + (x * Math.cos(a) - y * Math.sin(a)) / m.w, r.center[1] + (x * Math.sin(a) + y * Math.cos(a)) / m.h];
}

/** สิ่งที่วาดของวงกลมบนจอ: เส้นขอบนอก เส้นขอบในที่เริ่มนุ่ม จุดจับศูนย์กลาง/แกนนอน/แกนตั้ง */
export function radialShape(m: Mapper, r: RadialArea) {
  const ring = (k: number) => Array.from({ length: 72 }, (_, i) => m.toScreen(radialPoint(m, r, k, (i / 72) * Math.PI * 2)));
  return {
    outer: ring(1),
    inner: r.feather > 0 && r.feather < 100 ? ring(1 - r.feather / 100) : null,
    center: m.toScreen(r.center),
    xHandle: m.toScreen(radialPoint(m, r, 1, 0)),
    yHandle: m.toScreen(radialPoint(m, r, 1, Math.PI / 2)),
  };
}

export type RadialHandle = 'center' | 'x' | 'y';

/** ลากจุดจับของวงกลมไปที่จุด `p` บนจอ: ศูนย์กลาง = ย้าย · แกนนอน = รัศมีแนวนั้นและหมุน · แกนตั้ง = รัศมีอีกแนว */
export function moveRadial(m: Mapper, r: RadialArea, handle: RadialHandle, p: Px, grab?: { from: Px; center: [number, number] }): RadialArea {
  const at = m.toSource(p);
  if (handle === 'center') {
    // ย้ายตามระยะที่ลาก (จับตรงไหนของจุดจับก็ไม่กระโดด)
    const start = grab ? m.toSource(grab.from) : r.center;
    const base = grab?.center ?? r.center;
    return { ...r, center: pt([base[0] + at[0] - start[0], base[1] + at[1] - start[1]]) };
  }
  const v = sourceVector(m, r.center, at);
  if (handle === 'x') {
    const len = Math.max(Math.hypot(v.x, v.y), 1);
    return { ...r, radius: [round(len / m.w), r.radius[1]], angle: round((Math.atan2(v.y, v.x) * 180) / Math.PI) };
  }
  // ระยะตามแนวแกนตั้ง (ตั้งฉากกับแกนนอน)
  const a = (r.angle * Math.PI) / 180;
  const along = Math.max(Math.abs(-v.x * Math.sin(a) + v.y * Math.cos(a)), 1);
  return { ...r, radius: [r.radius[0], round(along / m.h)] };
}

/** สิ่งที่วาดของไล่ระดับบนจอ: เส้นสามเส้นตั้งฉากกับแนวไล่ (เต็มที่ กลาง จางหมด) ยาวพ้นภาพ + จุดจับ */
export function linearShape(m: Mapper, g: LinearArea, boxW: number, boxH: number) {
  const full = m.toScreen(g.full);
  const zero = m.toScreen(g.zero);
  const mid = { x: (full.x + zero.x) / 2, y: (full.y + zero.y) / 2 };
  const len = Math.hypot(zero.x - full.x, zero.y - full.y) || 1;
  // แนวตั้งฉากบนจอ (ภาพแสดงด้วยการหมุน + ย่อขยายเท่ากันทุกแนว เส้นตั้งฉากบนภาพเดิมจึงตั้งฉากบนจอด้วย)
  const n = { x: -(zero.y - full.y) / len, y: (zero.x - full.x) / len };
  const reach = Math.hypot(boxW, boxH);
  const line = (p: Px): [Px, Px] => [
    { x: p.x - n.x * reach, y: p.y - n.y * reach },
    { x: p.x + n.x * reach, y: p.y + n.y * reach },
  ];
  return { full, zero, mid, lines: [line(full), line(mid), line(zero)] as const };
}

export type LinearHandle = 'full' | 'zero' | 'mid';

/** ลากจุดจับของไล่ระดับ: จุดเต็มที่ / จุดจางหมด = ย้ายจุดนั้น · กลาง = ย้ายทั้งคู่ */
export function moveLinear(m: Mapper, g: LinearArea, handle: LinearHandle, p: Px, grab?: { from: Px; area: LinearArea }): LinearArea {
  const at = m.toSource(p);
  if (handle === 'mid') {
    const base = grab?.area ?? g;
    const start: [number, number] = grab ? m.toSource(grab.from) : [(g.full[0] + g.zero[0]) / 2, (g.full[1] + g.zero[1]) / 2];
    const [dx, dy] = [at[0] - start[0], at[1] - start[1]];
    return { ...g, full: pt([base.full[0] + dx, base.full[1] + dy]), zero: pt([base.zero[0] + dx, base.zero[1] + dy]) };
  }
  const next = { ...g, [handle]: pt(at) };
  // สองจุดซ้อนกัน = ไม่รู้แนว: คงจุดเดิมไว้
  return Math.hypot((next.full[0] - next.zero[0]) * m.w, (next.full[1] - next.zero[1]) * m.h) < 1 ? g : next;
}
