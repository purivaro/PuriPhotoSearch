import type { EditSettings, UnitRect } from '@/lib/ipc/types';
import { NEUTRAL, isNeutral } from './adjust';
import { maskIsActive } from './masks';
import { retouchIsActive } from './retouch';

/**
 * คณิตของหน้าแต่งภาพ — สูตรเดียวกับ `edit::Geometry` ใน Rust (มีเทสต์ค่าเดียวกันทั้งสองฝั่ง)
 * ลำดับ: ภาพหมุนตาม EXIF → หมุนทีละ 90° → ปรับเอียง (กรอบใหญ่สุดในภาพที่เอียง สัดส่วนเดิม) → ตัด
 */

export const IDENTITY: EditSettings = { rotate: 0, straighten: 0, crop: null, adjust: NEUTRAL };
export const FULL: UnitRect = { x: 0, y: 0, w: 1, h: 1 };
export const MAX_STRAIGHTEN = 45;
/** กรอบตัดเล็กสุด (สัดส่วนของแต่ละด้าน) — Rust: edit::MIN_CROP */
export const MIN_CROP = 0.02;

export function isIdentity(s: EditSettings): boolean {
  return isGeometryIdentity(s) && isNeutral(s.adjust) && !(s.masks ?? []).some(maskIsActive) && !retouchIsActive(s.retouch);
}

/** ไม่ได้ตัด หมุน หรือปรับเอียง */
export function isGeometryIdentity(s: EditSettings): boolean {
  const c = s.crop;
  return s.rotate === 0 && s.straighten === 0 && (!c || (c.x === 0 && c.y === 0 && c.w === 1 && c.h === 1));
}

/** ขนาดภาพหลังหมุนทีละ 90° */
export function rotatedSize(w: number, h: number, rotate: number) {
  return rotate % 180 === 90 ? { w: h, h: w } : { w, h };
}

/** กรอบหลังปรับเอียง = ภาพหลังหมุน × scale (สี่เหลี่ยมใหญ่สุดที่อยู่ในภาพที่เอียงได้ทั้งหมด) */
export function frameScale(w: number, h: number, degrees: number): number {
  const t = degrees * (Math.PI / 180); // เท่ากับ f64::to_radians ของ Rust ทุกบิต
  const sa = Math.abs(Math.sin(t));
  const ca = Math.abs(Math.cos(t));
  return Math.min(w / (w * ca + h * sa), h / (w * sa + h * ca));
}

/** ขนาดภาพที่แต่ง (พิกเซล) จากขนาดภาพเดิมหลังหมุนตาม EXIF */
export function outputSize(width: number, height: number, s: EditSettings) {
  const r = rotatedSize(Math.max(width, 1), Math.max(height, 1), s.rotate);
  const k = frameScale(r.w, r.h, s.straighten);
  const c = s.crop ?? FULL;
  return { w: Math.max(Math.round(c.w * k * r.w), 1), h: Math.max(Math.round(c.h * k * r.h), 1) };
}

/** ย้อน [`toFrame`]: จุดบนกรอบหลังปรับเอียง → จุดบนภาพเดิม (Rust: Geometry::to_rotated + quarter_inverse) */
export function fromFrame(width: number, height: number, s: EditSettings, fx: number, fy: number) {
  const r = rotatedSize(Math.max(width, 1), Math.max(height, 1), s.rotate);
  const k = frameScale(r.w, r.h, s.straighten);
  const t = s.straighten * (Math.PI / 180);
  const rx = (fx - 0.5) * k * r.w;
  const ry = (fy - 0.5) * k * r.h;
  const px = rx * Math.cos(t) + ry * Math.sin(t);
  const py = -rx * Math.sin(t) + ry * Math.cos(t);
  const u = px / r.w + 0.5;
  const v = py / r.h + 0.5;
  if (s.rotate === 90) return { x: v, y: 1 - u };
  if (s.rotate === 180) return { x: 1 - u, y: 1 - v };
  if (s.rotate === 270) return { x: 1 - v, y: u };
  return { x: u, y: v };
}

/** จุดบนภาพเดิม → จุดบนกรอบหลังปรับเอียง (ยังไม่ตัด) */
export function toFrame(width: number, height: number, s: EditSettings, u: number, v: number) {
  const [qu, qv] = s.rotate === 90 ? [1 - v, u] : s.rotate === 180 ? [1 - u, 1 - v] : s.rotate === 270 ? [v, 1 - u] : [u, v];
  const r = rotatedSize(Math.max(width, 1), Math.max(height, 1), s.rotate);
  const k = frameScale(r.w, r.h, s.straighten);
  const t = s.straighten * (Math.PI / 180);
  const px = (qu - 0.5) * r.w;
  const py = (qv - 0.5) * r.h;
  const rx = px * Math.cos(t) - py * Math.sin(t);
  const ry = px * Math.sin(t) + py * Math.cos(t);
  return { x: rx / (k * r.w) + 0.5, y: ry / (k * r.h) + 0.5 };
}

/** กรอบใบหน้าบนภาพเดิม → บนกรอบหลังปรับเอียง (กรอบตั้งตรงที่ครอบมุมทั้งสี่) */
export function boxToFrame(width: number, height: number, s: EditSettings, b: UnitRect): UnitRect {
  const corners: [number, number][] = [
    [b.x, b.y],
    [b.x + b.w, b.y],
    [b.x, b.y + b.h],
    [b.x + b.w, b.y + b.h],
  ];
  const pts = corners.map(([u, v]) => toFrame(width, height, s, u, v));
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
}

/** ใบหน้าที่เหลือในกรอบตัดไม่ถึงครึ่ง ถือว่าถูกตัดออก (Rust: Geometry::map_box) */
export function cutOut(face: UnitRect, crop: UnitRect | null): boolean {
  const c = crop ?? FULL;
  const x0 = Math.max(face.x, c.x);
  const y0 = Math.max(face.y, c.y);
  const x1 = Math.min(face.x + face.w, c.x + c.w);
  const y1 = Math.min(face.y + face.h, c.y + c.h);
  const visible = Math.max(0, x1 - x0) * Math.max(0, y1 - y0);
  return visible < face.w * face.h * 0.5;
}

/** หมุนกรอบตัดไปพร้อมภาพเมื่อกดหมุน 90° (ตามเข็ม = 1, ทวนเข็ม = −1) */
export function rotateCrop(c: UnitRect | null, direction: 1 | -1): UnitRect | null {
  if (!c) return null;
  return direction === 1 ? { x: 1 - c.y - c.h, y: c.x, w: c.h, h: c.w } : { x: c.y, y: 1 - c.x - c.w, w: c.h, h: c.w };
}

export function turn(rotate: EditSettings['rotate'], direction: 1 | -1): EditSettings['rotate'] {
  return (((rotate + direction * 90) % 360) + 360) % 360 as EditSettings['rotate'];
}

/**
 * กรอบตัดสัดส่วน `ratio` (กว้าง/สูง เป็นพิกเซลของภาพที่ได้) ใหญ่สุดที่อยู่ในกรอบได้ วางกลางที่จุดกลางของกรอบเดิม
 * `frameAspect` = กว้าง/สูงของกรอบหลังปรับเอียง
 */
export function fitAspect(current: UnitRect | null, ratio: number, frameAspect: number): UnitRect {
  const c = current ?? FULL;
  // h = w × frameAspect / ratio (เป็นสัดส่วนของกรอบ)
  let w = 1;
  let h = frameAspect / ratio;
  if (h > 1) {
    w = ratio / frameAspect;
    h = 1;
  }
  const cx = c.x + c.w / 2;
  const cy = c.y + c.h / 2;
  return { x: clamp(cx - w / 2, 0, 1 - w), y: clamp(cy - h / 2, 0, 1 - h), w, h };
}

export type Handle = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw' | 'move';

/**
 * ลากกรอบตัด: `dx`/`dy` เป็นสัดส่วนของกรอบ · `ratio` = ล็อกสัดส่วน (null = อิสระ)
 * ผลอยู่ในกรอบเสมอและไม่เล็กกว่า MIN_CROP
 */
export function dragCrop(start: UnitRect, handle: Handle, dx: number, dy: number, ratio: number | null, frameAspect: number): UnitRect {
  if (handle === 'move') {
    return { ...start, x: clamp(start.x + dx, 0, 1 - start.w), y: clamp(start.y + dy, 0, 1 - start.h) };
  }
  let x0 = start.x;
  let y0 = start.y;
  let x1 = start.x + start.w;
  let y1 = start.y + start.h;
  if (handle.includes('w')) x0 = clamp(x0 + dx, 0, x1 - MIN_CROP);
  if (handle.includes('e')) x1 = clamp(x1 + dx, x0 + MIN_CROP, 1);
  if (handle.includes('n')) y0 = clamp(y0 + dy, 0, y1 - MIN_CROP);
  if (handle.includes('s')) y1 = clamp(y1 + dy, y0 + MIN_CROP, 1);
  if (ratio) {
    // ล็อกสัดส่วน: ด้านที่ลากเป็นตัวตั้ง แล้วปรับอีกด้านจากมุมตรงข้าม (ไม่ล้นกรอบ)
    const k = frameAspect / ratio;
    const horizontal = handle === 'e' || handle === 'w';
    let w = x1 - x0;
    let h = y1 - y0;
    if (horizontal || (handle.length === 2 && w * k >= h)) h = w * k;
    else w = h / k;
    const anchorX = handle.includes('w') ? x1 : x0;
    const anchorY = handle.includes('n') ? y1 : y0;
    const roomX = handle.includes('w') ? anchorX : 1 - anchorX;
    const roomY = handle.includes('n') ? anchorY : 1 - anchorY;
    const fit = Math.min(1, roomX / w, roomY / h);
    w *= fit;
    h *= fit;
    if (handle === 'n' || handle === 's') {
      const cx = (x0 + x1) / 2;
      x0 = clamp(cx - w / 2, 0, 1 - w);
    } else {
      x0 = handle.includes('w') ? anchorX - w : anchorX;
    }
    if (handle === 'e' || handle === 'w') {
      const cy = (y0 + y1) / 2;
      y0 = clamp(cy - h / 2, 0, 1 - h);
    } else {
      y0 = handle.includes('n') ? anchorY - h : anchorY;
    }
    return { x: x0, y: y0, w, h };
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

function clamp(v: number, lo: number, hi: number) {
  return Math.min(Math.max(v, lo), Math.max(lo, hi));
}
