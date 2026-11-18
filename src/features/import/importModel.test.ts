import { describe, expect, it } from 'vitest';
import type { EventDraft, ImportPreview } from '@/lib/ipc/types';
import { buildCommit, draftsForMode, initialEdits, isSelectable, summarize, type DraftEdit } from './importModel';

function draft(partial: Partial<EventDraft> & { draftId: string }): EventDraft {
  return {
    kind: 'folder',
    folderName: partial.draftId,
    displayPath: `/root/${partial.draftId}`,
    suggestedTitle: partial.draftId,
    suggestedDate: null,
    dateSource: 'none',
    photoCount: 10,
    newCount: 10,
    existingCount: 0,
    unsupportedCount: 0,
    readable: true,
    unreadableReason: null,
    existingEventId: null,
    existingEventTitle: null,
    defaultSelected: true,
    hasSample: true,
    ...partial,
  };
}

const preview: ImportPreview = {
  previewId: 'pv',
  rootPath: '/root',
  rootName: 'root',
  suggestedMode: 'child_events',
  single: draft({ draftId: 'whole', kind: 'whole', photoCount: 59, newCount: 44 }),
  children: [
    draft({ draftId: 'a', suggestedTitle: 'งานอบรม AI', suggestedDate: '2026-11-18', dateSource: 'folder_name', photoCount: 20, newCount: 20 }),
    draft({ draftId: 'b', photoCount: 15, newCount: 0, existingCount: 15, existingEventId: 'ev1', existingEventTitle: 'ชื่อที่แก้ไว้', defaultSelected: false }),
    draft({ draftId: 'c', readable: false, unreadableReason: 'ไม่มีสิทธิ์', photoCount: 0, newCount: 0 }),
    draft({ draftId: 'd', photoCount: 0, newCount: 0, defaultSelected: false }),
  ],
  rootLoose: draft({ draftId: 'loose', kind: 'root_loose', photoCount: 24, newCount: 24, defaultSelected: false }),
  unsupportedByExt: {},
  expiresAt: '2026-11-18T10:00:00Z',
};

describe('หน้าตรวจรายการก่อนเพิ่ม', () => {
  it('โหมดหลายงานมีแถวรูปตรงโฟลเดอร์แม่เสมอ โหมดงานเดียวมีแถวเดียว', () => {
    expect(draftsForMode(preview, 'child_events').map((d) => d.draftId)).toEqual(['a', 'b', 'c', 'd', 'loose']);
    expect(draftsForMode(preview, 'single_event').map((d) => d.draftId)).toEqual(['whole']);
  });

  it('แถวอ่านไม่ได้และงานเดิมที่ไม่มีรูปใหม่เลือกไม่ได้', () => {
    const byId = Object.fromEntries(preview.children.map((d) => [d.draftId, d]));
    expect(isSelectable(byId.a as EventDraft)).toBe(true);
    expect(isSelectable(byId.b as EventDraft)).toBe(false);
    expect(isSelectable(byId.c as EventDraft)).toBe(false);
    expect(isSelectable({ ...(byId.b as EventDraft), newCount: 3 })).toBe(true);
  });

  it('ค่าเริ่มต้นใช้ชื่อกิจกรรมเดิมถ้ามี และไม่เลือกแถวที่เลือกไม่ได้', () => {
    const edits = initialEdits(preview);
    expect(edits.a).toEqual({ selected: true, title: 'งานอบรม AI', eventDate: '2026-11-18' });
    expect(edits.b?.title).toBe('ชื่อที่แก้ไว้');
    expect(edits.c?.selected).toBe(false);
    expect(edits.loose?.selected).toBe(false);
  });

  it('สรุปแยกรูปที่จะเพิ่มใหม่ รูปที่มีอยู่แล้ว และรูปที่ยังไม่เลือก', () => {
    const edits = initialEdits(preview);
    const rows = draftsForMode(preview, 'child_events');
    expect(summarize(rows, edits)).toEqual({ eventCount: 1, newPhotos: 20, existingPhotos: 0, unselectedPhotos: 24, missingTitles: 0, emptySelected: 0 });
    edits.loose = { ...(edits.loose as DraftEdit), selected: true };
    edits.a = { ...(edits.a as DraftEdit), title: '   ' };
    const next = summarize(rows, edits);
    expect(next.eventCount).toBe(2);
    expect(next.newPhotos).toBe(44);
    expect(next.unselectedPhotos).toBe(0);
    expect(next.missingTitles).toBe(1);
  });

  it('ส่งเฉพาะแถวที่เลือกของโหมดปัจจุบัน และตัดช่องว่างของชื่อ', () => {
    const edits = initialEdits(preview);
    edits.a = { selected: true, title: '  งานอบรม AI รุ่น 2 ', eventDate: null };
    edits.c = { selected: true, title: 'x', eventDate: null }; // อ่านไม่ได้ ต้องไม่ถูกส่ง
    const args = buildCommit(preview, 'child_events', edits, { requestId: 'r1', analyzeFaces: false, watch: true });
    expect(args.drafts).toEqual([{ draftId: 'a', title: 'งานอบรม AI รุ่น 2', eventDate: null }]);
    expect(args.mode).toBe('child_events');
    expect(args.requestId).toBe('r1');
    const single = buildCommit(preview, 'single_event', edits, { requestId: 'r2', analyzeFaces: false, watch: true });
    expect(single.drafts.map((d) => d.draftId)).toEqual(['whole']);
  });
});
