import { formatCount } from '@/lib/format';
import type { EditMask, EditSettings } from '@/lib/ipc/types';
import { gradingIsActive, hasCurves, hasHsl, hasLevels, hasVignette, type SliderKey } from './adjust';
import { isGeometryIdentity } from './editGeometry';
import { maskIsActive } from './masks';

/** ชื่อของบริเวณที่ปรับ (แผงปรับเฉพาะส่วน · สรุปการแต่ง) */
export function maskName(m: EditMask): string {
  const n = m.area.boxes.length;
  return `ใบหน้า ${formatCount(n)} คน${m.area.skin ? ' · เฉพาะผิว' : ''}${m.invert ? ' · กลับด้าน' : ''}`;
}

const LIGHT: [SliderKey, string][] = [
  ['exposure', 'ค่าแสง'],
  ['contrast', 'คอนทราสต์'],
  ['highlights', 'ส่วนสว่าง'],
  ['shadows', 'เงา'],
  ['whites', 'จุดขาว'],
  ['blacks', 'จุดดำ'],
  ['texture', 'พื้นผิว'],
  ['clarity', 'ความชัด'],
  ['dehaze', 'ลดหมอก'],
];
const COLOR: [SliderKey, string][] = [
  ['temperature', 'อุณหภูมิสี'],
  ['tint', 'โทนสี'],
  ['vibrance', 'ความสด'],
  ['saturation', 'ความอิ่ม'],
];

const value = (key: string, v: number) => (key === 'exposure' ? `${v > 0 ? '+' : ''}${v.toFixed(2)} EV` : `${v > 0 ? '+' : ''}${v}`);

function list(s: EditSettings, items: [SliderKey, string][], extra: (string | false)[]): string | null {
  const parts = items.filter(([k]) => s.adjust[k] !== 0).map(([k, label]) => `${label} ${value(k, s.adjust[k])}`);
  for (const e of extra) if (e) parts.push(e);
  return parts.length > 0 ? parts.join(' · ') : null;
}

/** สรุปการแต่งแต่ละส่วนเป็นภาษาผู้ใช้ (null = ส่วนนั้นไม่ได้แต่ง) */
export function describeEdit(s: EditSettings) {
  const geometry: string[] = [];
  if (s.rotate !== 0) geometry.push(`หมุน ${s.rotate}°`);
  if (s.straighten !== 0) geometry.push(`เอียง ${s.straighten.toFixed(1)}°`);
  if (s.crop && !isGeometryIdentity({ ...s, rotate: 0, straighten: 0 })) geometry.push('ตัดภาพ');
  const masks = (s.masks ?? []).filter(maskIsActive);
  return {
    geometry: geometry.length > 0 ? geometry.join(' · ') : null,
    masks: masks.length > 0 ? `${formatCount(masks.length)} บริเวณ (${masks.map((m) => (m.area.skin ? 'ผิวหน้า' : 'ใบหน้า')).join(', ')})` : null,
    light: list(s, LIGHT, [hasLevels(s.adjust) && 'Levels', hasCurves(s.adjust) && 'เส้นโค้งโทน', hasVignette(s.adjust) && 'Vignette']),
    color: list(s, COLOR, [hasHsl(s.adjust) && 'สีแยกช่วง (HSL)', gradingIsActive(s.adjust.grading) && 'Color Grading']),
  };
}
