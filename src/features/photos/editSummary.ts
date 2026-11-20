import type { EditSettings } from '@/lib/ipc/types';
import { isGeometryIdentity } from './editGeometry';

const LIGHT: [keyof EditSettings['adjust'], string][] = [
  ['exposure', 'ค่าแสง'],
  ['contrast', 'คอนทราสต์'],
  ['highlights', 'ส่วนสว่าง'],
  ['shadows', 'เงา'],
  ['whites', 'จุดขาว'],
  ['blacks', 'จุดดำ'],
];
const COLOR: [keyof EditSettings['adjust'], string][] = [
  ['temperature', 'อุณหภูมิสี'],
  ['tint', 'โทนสี'],
  ['vibrance', 'ความสด'],
  ['saturation', 'ความอิ่ม'],
];

const value = (key: string, v: number) => (key === 'exposure' ? `${v > 0 ? '+' : ''}${v.toFixed(2)} EV` : `${v > 0 ? '+' : ''}${v}`);

function list(s: EditSettings, items: [keyof EditSettings['adjust'], string][]): string | null {
  const parts = items.filter(([k]) => s.adjust[k] !== 0).map(([k, label]) => `${label} ${value(k, s.adjust[k])}`);
  return parts.length > 0 ? parts.join(' · ') : null;
}

/** สรุปการแต่งแต่ละส่วนเป็นภาษาผู้ใช้ (null = ส่วนนั้นไม่ได้แต่ง) */
export function describeEdit(s: EditSettings) {
  const geometry: string[] = [];
  if (s.rotate !== 0) geometry.push(`หมุน ${s.rotate}°`);
  if (s.straighten !== 0) geometry.push(`เอียง ${s.straighten.toFixed(1)}°`);
  if (s.crop && !isGeometryIdentity({ ...s, rotate: 0, straighten: 0 })) geometry.push('ตัดภาพ');
  return { geometry: geometry.length > 0 ? geometry.join(' · ') : null, light: list(s, LIGHT), color: list(s, COLOR) };
}
