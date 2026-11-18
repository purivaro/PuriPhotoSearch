import { describe, expect, it } from 'vitest';
import type { ReviewOverview } from '@/lib/ipc/types';
import { defaultTab, levelLabel, photoIdsOf, queueGroups, resolveGroup, selectAllFaces, selectedFaceIds, selectionCheckState, toggleFace } from './reviewModel';

const faces = [
  { faceId: 'f1', photoId: 'p1' },
  { faceId: 'f2', photoId: 'p1' },
  { faceId: 'f3', photoId: 'p2' },
];

function overview(partial: Partial<ReviewOverview>): ReviewOverview {
  return { reviewRevision: 1, unnamedFaceCount: 0, suggested: [], clusters: [], singleFaceCount: 0, unexaminedFaceCount: 0, photosPendingAnalysis: 0, ...partial };
}

describe('การเลือกใบหน้าในคิวตรวจ', () => {
  it('ค่าเริ่มต้นเลือกทุกใบที่โหลด และเอาออกทีละใบได้', () => {
    const all = selectAllFaces(faces);
    expect(selectedFaceIds(faces, all)).toEqual(['f1', 'f2', 'f3']);
    expect(selectionCheckState(faces, all)).toBe(true);
    const without = toggleFace(all, 'f2');
    expect(selectedFaceIds(faces, without)).toEqual(['f1', 'f3']);
    expect(selectionCheckState(faces, without)).toBe('indeterminate');
    expect(all.has('f2')).toBe(true); // ไม่แก้ของเดิม
    expect(selectionCheckState(faces, new Set())).toBe(false);
  });

  it('ไม่ส่งใบหน้าที่ไม่ได้อยู่ในรายการที่แสดง แม้ค้างอยู่ใน selection', () => {
    const stale = new Set(['f1', 'f9-from-another-group']);
    expect(selectedFaceIds(faces, stale)).toEqual(['f1']);
  });

  it('ลำดับรูปของตัวดูภาพไม่ซ้ำ แม้รูปเดียวมีหลายใบหน้า', () => {
    expect(photoIdsOf(faces)).toEqual(['p1', 'p2']);
  });
});

describe('การเลื่อนไปกลุ่มถัดไป', () => {
  const groups = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
  it('เปิดครั้งแรกเลือกกลุ่มแรก', () => {
    expect(resolveGroup(groups, null)).toBe('a');
    expect(resolveGroup([], null)).toBeNull();
  });
  it('กลุ่มเดิมยังเหลือใบหน้า → อยู่ที่เดิม', () => {
    expect(resolveGroup(groups, { id: 'b', index: 1 })).toBe('b');
  });
  it('กลุ่มเดิมหมดแล้ว → กลุ่มที่เลื่อนขึ้นมาแทนตำแหน่งเดิม', () => {
    expect(resolveGroup([{ id: 'a' }, { id: 'c' }], { id: 'b', index: 1 })).toBe('c');
    expect(resolveGroup([{ id: 'a' }], { id: 'c', index: 2 })).toBe('a');
    expect(resolveGroup([], { id: 'a', index: 0 })).toBeNull();
  });
});

describe('คิวและแท็บ', () => {
  const person = { id: 'person-1', displayName: 'ก', coverFaceId: null, coverAsset: null, nickname: null, confirmedPhotoCount: 3, aiPhotoCount: 0, pinned: false, groups: [], archived: false, recordVersion: 1 };
  const data = overview({
    suggested: [{ person, faceCount: 4, photoCount: 4, sampleFaceIds: ['f1'] }],
    clusters: [{ clusterId: 'c1', faceCount: 2, sampleFaceIds: ['f2', 'f3'] }],
  });
  it('แปลงเป็นรายการคิวตามแท็บ โดยอ้างด้วย ID ไม่ใช่ชื่อ', () => {
    expect(queueGroups(data, 'suggested')).toEqual([{ id: 'person-1', kind: 'person', faceCount: 4, sampleFaceIds: ['f1'] }]);
    expect(queueGroups(data, 'unknown')).toEqual([{ id: 'c1', kind: 'cluster', faceCount: 2, sampleFaceIds: ['f2', 'f3'] }]);
  });
  it('แท็บเริ่มต้นคือแท็บที่มีงาน', () => {
    expect(defaultTab(data)).toBe('suggested');
    expect(defaultTab(overview({ clusters: data.clusters }))).toBe('unknown');
    expect(defaultTab(overview({}))).toBe('suggested');
  });
  it('ป้ายระดับเป็นคำ ไม่ใช่ตัวเลข', () => {
    expect(levelLabel('close')).toBe('ใกล้เคียงมาก');
    expect(levelLabel('check')).toBe('ควรตรวจสอบ');
  });
});
