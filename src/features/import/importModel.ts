import type { CommitImportArgs, EventDraft, Id, ImportPreview, SourceMode } from '@/lib/ipc/types';

/** ค่าที่ผู้ใช้แก้ในหน้าตรวจรายการ ต่อหนึ่งแถว */
export interface DraftEdit {
  selected: boolean;
  title: string;
  /** YYYY-MM-DD หรือ null = ยังไม่ระบุวันที่ */
  eventDate: string | null;
}

export type DraftEdits = Record<Id, DraftEdit>;

/** แถวที่แสดงในแต่ละโหมด รวมแถว "รูปที่อยู่ในโฟลเดอร์นี้" เพื่อไม่ให้รูปตรงโฟลเดอร์แม่ตกหล่นเงียบ ๆ */
export function draftsForMode(preview: ImportPreview, mode: SourceMode): EventDraft[] {
  if (mode === 'single_event') return [preview.single];
  return preview.rootLoose ? [...preview.children, preview.rootLoose] : preview.children;
}

export function allDrafts(preview: ImportPreview): EventDraft[] {
  return [preview.single, ...preview.children, ...(preview.rootLoose ? [preview.rootLoose] : [])];
}

/** รูปใหม่ของแถวนี้ (รวมไฟล์ RAW เมื่อเลือกรวม RAW) */
export function newOf(draft: EventDraft, includeRaw: boolean): number {
  return draft.newCount + (includeRaw ? draft.rawNewCount : 0);
}

/** เลือกแถวนี้ได้หรือไม่: อ่านไม่ได้เลือกไม่ได้; งานที่อยู่ในคลังแล้วเลือกได้เมื่อมีรูปใหม่ให้ตรวจ (หรือเลือกรวม RAW ที่ยังไม่ได้นำเข้า) */
export function isSelectable(draft: EventDraft, includeRaw = false): boolean {
  if (!draft.readable) return false;
  if (draft.existingEventId) return newOf(draft, includeRaw) > 0;
  return true;
}

/**
 * ค่าเริ่มต้นของ "รวมไฟล์ RAW" (DECISIONS ข้อ 25): เปิดเมื่อโฟลเดอร์มีแต่ RAW (ถ่าย RAW อย่างเดียว) ·
 * มีทั้ง JPEG และ RAW = ปิดไว้ก่อน เพราะ JPEG มักเป็นไฟล์ที่ export มาจาก RAW เดียวกัน รวมแล้วรูปจะซ้ำ
 */
export function rawDefault(preview: ImportPreview): boolean {
  return preview.single.rawCount > 0 && preview.single.photoCount === 0;
}

export function initialEdits(preview: ImportPreview): DraftEdits {
  const edits: DraftEdits = {};
  for (const draft of allDrafts(preview)) {
    edits[draft.draftId] = {
      selected: draft.defaultSelected && isSelectable(draft),
      title: draft.existingEventTitle ?? draft.suggestedTitle,
      eventDate: draft.suggestedDate,
    };
  }
  return edits;
}

export interface ImportSummary {
  /** กิจกรรมที่จะเพิ่ม/ตรวจรูปใหม่ */
  eventCount: number;
  /** รูปที่จะเพิ่มใหม่จริง */
  newPhotos: number;
  /** รูปของแถวที่เลือกซึ่งอยู่ในคลังแล้ว (ไม่เพิ่มซ้ำ) */
  existingPhotos: number;
  /** รูปในแถวที่เลือกได้แต่ยังไม่ได้เลือก */
  unselectedPhotos: number;
  /** แถวที่เลือกแต่ชื่อว่าง */
  missingTitles: number;
  /** แถวที่เลือกแต่ไม่มีรูปเลย (เชื่อมไว้รอรูป) */
  emptySelected: number;
}

export function summarize(drafts: EventDraft[], edits: DraftEdits, includeRaw = false): ImportSummary {
  const summary: ImportSummary = { eventCount: 0, newPhotos: 0, existingPhotos: 0, unselectedPhotos: 0, missingTitles: 0, emptySelected: 0 };
  for (const draft of drafts) {
    const edit = edits[draft.draftId];
    if (edit?.selected && isSelectable(draft, includeRaw)) {
      summary.eventCount += 1;
      summary.newPhotos += newOf(draft, includeRaw);
      summary.existingPhotos += draft.existingCount + (includeRaw ? draft.rawCount - draft.rawNewCount : 0);
      if (!edit.title.trim()) summary.missingTitles += 1;
      if (draft.photoCount === 0 && !(includeRaw && draft.rawCount > 0)) summary.emptySelected += 1;
    } else if (isSelectable(draft, includeRaw)) {
      summary.unselectedPhotos += newOf(draft, includeRaw);
    }
  }
  return summary;
}

export function buildCommit(
  preview: ImportPreview,
  mode: SourceMode,
  edits: DraftEdits,
  options: { requestId: Id; analyzeFaces: boolean; watch: boolean; includeRaw?: boolean },
): CommitImportArgs {
  const includeRaw = options.includeRaw ?? false;
  const drafts = draftsForMode(preview, mode)
    .filter((draft) => edits[draft.draftId]?.selected && isSelectable(draft, includeRaw))
    .map((draft) => {
      const edit = edits[draft.draftId] as DraftEdit;
      return { draftId: draft.draftId, title: edit.title.trim(), eventDate: edit.eventDate };
    });
  return { previewId: preview.previewId, requestId: options.requestId, mode, drafts, analyzeFaces: options.analyzeFaces, watch: options.watch, includeRaw };
}
