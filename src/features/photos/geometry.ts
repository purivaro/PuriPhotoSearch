/** คณิตของตัวดูภาพ: หาสี่เหลี่ยมที่ภาพถูกวาดจริง เพื่อให้กรอบใบหน้าตรงทุกระดับ zoom และทุกขนาดหน้าต่าง */

export interface Size {
  w: number;
  h: number;
}
export interface Point {
  x: number;
  y: number;
}
export interface Rect extends Point, Size {}
/** สัดส่วน [0,1] บนภาพที่หมุนตาม EXIF แล้ว */
export type NormalizedBox = Rect;

export const MIN_ZOOM = 1;
export const MAX_ZOOM = 8;
/** กรอบที่เล็กกว่านี้ (สัดส่วนของด้านภาพ) ถือว่าเป็นการคลิกพลาด ไม่ใช่การลากกรอบ */
export const MIN_BOX_SIDE = 0.012;

/** อัตราย่อที่ทำให้ภาพทั้งใบพอดีพื้นที่ (ไม่ขยายเกินขนาดจริงของพื้นที่) */
export function fitScale(container: Size, image: Size): number {
  if (image.w <= 0 || image.h <= 0 || container.w <= 0 || container.h <= 0) return 1;
  return Math.min(container.w / image.w, container.h / image.h);
}

/** สี่เหลี่ยมภาพบนจอ: zoom=1 คือพอดีหน้าต่างและอยู่กลาง; pan เป็นระยะเลื่อนจากกึ่งกลางเป็นพิกเซล */
export function imageRect(container: Size, image: Size, zoom: number, pan: Point): Rect {
  const scale = fitScale(container, image) * zoom;
  const w = image.w * scale;
  const h = image.h * scale;
  return { x: (container.w - w) / 2 + pan.x, y: (container.h - h) / 2 + pan.y, w, h };
}

/** จำกัดการเลื่อนไม่ให้ภาพหลุดขอบ: ด้านที่ภาพเล็กกว่าพื้นที่จะถูกตรึงไว้กลาง */
export function clampPan(container: Size, image: Size, zoom: number, pan: Point): Point {
  const scale = fitScale(container, image) * zoom;
  const maxX = Math.max(0, (image.w * scale - container.w) / 2);
  const maxY = Math.max(0, (image.h * scale - container.h) / 2);
  return { x: Math.min(maxX, Math.max(-maxX, pan.x)), y: Math.min(maxY, Math.max(-maxY, pan.y)) };
}

export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

/** เปลี่ยน zoom โดยให้จุดใต้ตัวชี้ (พิกัดใน container) อยู่ที่เดิม */
export function zoomAround(container: Size, image: Size, zoom: number, pan: Point, nextZoom: number, focus: Point): { zoom: number; pan: Point } {
  const target = clampZoom(nextZoom);
  const before = imageRect(container, image, zoom, pan);
  if (before.w <= 0 || before.h <= 0) return { zoom: target, pan: { x: 0, y: 0 } };
  const u = (focus.x - before.x) / before.w;
  const v = (focus.y - before.y) / before.h;
  const centered = imageRect(container, image, target, { x: 0, y: 0 });
  const nextPan = { x: focus.x - u * centered.w - centered.x, y: focus.y - v * centered.h - centered.y };
  return { zoom: target, pan: clampPan(container, image, target, nextPan) };
}

/** พิกัดจอ → สัดส่วนบนภาพ (ตัดให้อยู่ในภาพ) */
export function toNormalized(rect: Rect, point: Point): Point {
  const clamp = (n: number) => Math.min(1, Math.max(0, n));
  if (rect.w <= 0 || rect.h <= 0) return { x: 0, y: 0 };
  return { x: clamp((point.x - rect.x) / rect.w), y: clamp((point.y - rect.y) / rect.h) };
}

/** กรอบจากจุดเริ่มและจุดปลายของการลาก (ลากทิศใดก็ได้) */
export function boxFromDrag(start: Point, end: Point): NormalizedBox {
  const x = Math.min(start.x, end.x);
  const y = Math.min(start.y, end.y);
  return { x, y, w: Math.abs(end.x - start.x), h: Math.abs(end.y - start.y) };
}

export function isUsableBox(box: NormalizedBox): boolean {
  return box.w >= MIN_BOX_SIDE && box.h >= MIN_BOX_SIDE;
}

/** สัดส่วน → พิกัดจอ ใช้ตรวจว่ากรอบยังชี้ตำแหน่งเดิมหลัง zoom/resize */
export function boxToScreen(rect: Rect, box: NormalizedBox): Rect {
  return { x: rect.x + box.x * rect.w, y: rect.y + box.y * rect.h, w: box.w * rect.w, h: box.h * rect.h };
}

export function percent(n: number): string {
  return `${(n * 100).toFixed(4)}%`;
}
