import { describe, expect, it } from 'vitest';
import type { EditMask } from '@/lib/ipc/types';
import { NEUTRAL } from './adjust';
import { MAX_MASKS, addMasks, isSubjectMask } from './masks';

const face = (id: string): EditMask => ({ id, area: { kind: 'faces', boxes: [[0.4, 0.3, 0.2, 0.3]], skin: true }, adjust: NEUTRAL, amount: 100, invert: false });
const subject = (id: string, texture = -35): EditMask => ({ id, area: { kind: 'subject' }, adjust: { ...NEUTRAL, texture }, amount: 90, invert: false });

describe('addMasks (ใช้ preset ที่มี Mask ตัวคน)', () => {
  it('keeps the photo’s own masks and adds the preset’s', () => {
    expect(addMasks([face('f')], [subject('lr-1')]).map((m) => m.id)).toEqual(['f', 'lr-1']);
  });

  it('replaces a mask with the same id instead of stacking it', () => {
    const once = addMasks([face('f')], [subject('lr-1', -35)]);
    const twice = addMasks(once, [subject('lr-1', -20)]);
    expect(twice.map((m) => m.id)).toEqual(['f', 'lr-1']);
    expect(twice[1]?.adjust.texture).toBe(-20);
  });

  it('drops the oldest masks when the photo would have too many', () => {
    const many = Array.from({ length: MAX_MASKS }, (_, i) => face(`f${i}`));
    const out = addMasks(many, [subject('lr-1')]);
    expect(out).toHaveLength(MAX_MASKS);
    expect(out[0]?.id).toBe('f1');
    expect(out.at(-1)?.id).toBe('lr-1');
  });

  it('tells subject masks apart from face masks', () => {
    expect([face('f'), subject('s')].map(isSubjectMask)).toEqual([false, true]);
  });
});
