import { describe, expect, it } from 'vitest';
import { EMPTY_SELECTION, isSelected, selectLoaded, selectRange, selectSnapshot, selectionCount, toSelectionSpec, toggle } from './selection';

const order = ['a', 'b', 'c', 'd', 'e', 'f'];

describe('selection', () => {
  it('ติ๊กและเอาออกทีละรูป', () => {
    let s = toggle(EMPTY_SELECTION, 'b');
    expect(isSelected(s, 'b')).toBe(true);
    expect(selectionCount(s)).toBe(1);
    s = toggle(s, 'b');
    expect(selectionCount(s)).toBe(0);
  });

  it('Shift เลือกช่วงตามลำดับที่แสดง ทั้งไปข้างหน้าและย้อนหลัง', () => {
    let s = toggle(EMPTY_SELECTION, 'b');
    s = selectRange(s, order, 'e');
    expect(toSelectionSpec(s)).toEqual({ kind: 'explicit', photoIds: ['b', 'c', 'd', 'e'] });
    let back = toggle(EMPTY_SELECTION, 'e');
    back = selectRange(back, order, 'c');
    expect(selectionCount(back)).toBe(3);
    expect(isSelected(back, 'b')).toBe(false);
  });

  it('Shift โดยไม่มีจุดยึดเลือกเฉพาะใบที่คลิก', () => {
    const s = selectRange(EMPTY_SELECTION, order, 'd');
    expect(toSelectionSpec(s)).toEqual({ kind: 'explicit', photoIds: ['d'] });
  });

  it('เลือกที่โหลดแล้วไม่ใช่เลือกผลทั้งหมด', () => {
    const s = selectLoaded(EMPTY_SELECTION, ['a', 'b', 'c']);
    expect(s.kind).toBe('explicit');
    expect(selectionCount(s)).toBe(3);
  });

  it('ผลค้นหาทั้งหมดนับจาก snapshot และเอาออกรายรูปได้', () => {
    let s = selectSnapshot('tok', 1200);
    expect(selectionCount(s)).toBe(1200);
    expect(isSelected(s, 'ยังไม่ได้โหลด')).toBe(true);
    s = toggle(s, 'c');
    expect(isSelected(s, 'c')).toBe(false);
    expect(selectionCount(s)).toBe(1199);
    expect(toSelectionSpec(s)).toEqual({ kind: 'snapshot', selectionToken: 'tok', excludedPhotoIds: ['c'] });
    s = selectLoaded(s, order);
    expect(selectionCount(s)).toBe(1200);
  });
});
