import { describe, expect, it } from 'vitest';
import type { EditMask, ModelStatus, PersonPart, SubjectStatus } from '@/lib/ipc/types';
import { NEUTRAL } from './adjust';
import { maskName } from './editSummary';
import { MAX_MASKS, addMasks, isPeopleMask, isPortableMask, isSubjectMask } from './masks';
import { missingFor } from './useSubjectMap';

const face = (id: string): EditMask => ({ id, area: { kind: 'faces', boxes: [[0.4, 0.3, 0.2, 0.3]], skin: true }, adjust: NEUTRAL, amount: 100, invert: false });
const people = (id: string, parts: PersonPart[], invert = false): EditMask => ({ id, area: { kind: 'people', parts }, adjust: { ...NEUTRAL, exposure: 0.3 }, amount: 100, invert });
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

describe('Mask ส่วนของคน (DECISIONS ข้อ 43)', () => {
  it('is portable like a subject mask, unlike a face mask', () => {
    const all = [face('f'), subject('s'), people('p', ['hair'])];
    expect(all.map(isPeopleMask)).toEqual([false, false, true]);
    expect(all.map(isPortableMask)).toEqual([false, true, true]);
  });

  it('names the parts it joins', () => {
    expect(maskName(people('p', ['hair']))).toBe('ผม (AI)');
    expect(maskName(people('p', ['faceSkin', 'bodySkin'], true))).toBe('ผิวหน้า + ผิวตัว (AI) · กลับด้าน');
  });

  it('asks only for the models the photo still lacks', () => {
    const model = (state: ModelStatus['state'], name: string): ModelStatus => ({
      state,
      reason: '',
      modelName: name,
      licenseNote: '',
      downloadSource: '',
      downloadSizeMb: 10,
      installJobId: null,
    });
    const status: SubjectStatus = { ...model('ready', 'segment'), fill: model('not_installed', 'fill'), parts: model('not_installed', 'parts') };
    expect(missingFor(status, { body: false, parts: false })).toEqual([]);
    expect(missingFor(status, { body: false, parts: true }).map((m) => m.modelName)).toEqual(['parts']);
    expect(missingFor(status, { body: true, parts: true }).map((m) => m.modelName)).toEqual(['fill', 'parts']);
  });
});
