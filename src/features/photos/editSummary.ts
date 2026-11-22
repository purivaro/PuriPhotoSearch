import { formatCount } from '@/lib/format';
import type { PersonPart, EditMask, EditSettings } from '@/lib/ipc/types';
import { gradingIsActive, hasCurves, hasHsl, hasLevels, hasSharpen, hasVignette, type SliderKey } from './adjust';
import { isGeometryIdentity } from './editGeometry';
import { maskIsActive } from './masks';
import { describeRetouch } from './retouch';

/** ชื่อของบริเวณที่ปรับ (แผงปรับเฉพาะส่วน · สรุปการแต่ง) */
/** ชื่อส่วนของคน (Mask ส่วนของคน — DECISIONS ข้อ 43) */
export const PART_LABELS: Record<PersonPart, string> = { hair: 'ผม', faceSkin: 'ผิวหน้า', bodySkin: 'ผิวตัว', clothes: 'เสื้อผ้า' };

export function maskName(m: EditMask): string {
  const invert = m.invert ? ' · กลับด้าน' : '';
  if (m.area.kind === 'subject') return `ตัวคน (AI)${invert}`;
  if (m.area.kind === 'people') return `${m.area.parts.map((p) => PART_LABELS[p]).join(' + ')} (AI)${invert}`;
  if (m.area.kind === 'sky') return `ท้องฟ้า (AI)${invert}`;
  return `ใบหน้า ${formatCount(m.area.boxes.length)} คน${m.area.skin ? ' · เฉพาะผิว' : ''}${invert}`;
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
    masks:
      masks.length > 0
        ? `${formatCount(masks.length)} บริเวณ (${masks
            .map((m) =>
              m.area.kind === 'subject'
                ? 'ตัวคน'
                : m.area.kind === 'people'
                  ? m.area.parts.map((p) => PART_LABELS[p]).join('+')
                  : m.area.kind === 'sky'
                    ? 'ท้องฟ้า'
                    : m.area.skin
                    ? 'ผิวหน้า'
                    : 'ใบหน้า',
            )
            .join(', ')})`
        : null,
    retouch: describeRetouch(s.retouch),
    light: list(s, LIGHT, [hasLevels(s.adjust) && 'Levels', hasCurves(s.adjust) && 'เส้นโค้งโทน', hasVignette(s.adjust) && 'Vignette', hasSharpen(s.adjust) && 'ความคม']),
    color: list(s, COLOR, [hasHsl(s.adjust) && 'สีแยกช่วง (HSL)', gradingIsActive(s.adjust.grading) && 'Color Grading']),
  };
}
