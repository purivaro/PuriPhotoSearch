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

/** เลือกแถวนี้ได้หรือไม่: อ่านไม่ได้เลือกไม่ได้; งานที่อยู่ในคลังแล้วเลือกได้เมื่อมีรูปใหม่ให้ตรวจ */
export function isSelectable(draft: EventDraft): boolean {
  if (!draft.readable) return false;
  if (draft.existingEventId) return draft.newCount > 0;
  return true;
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

export function summarize(drafts: EventDraft[], edits: DraftEdits): ImportSummary {
  const summary: ImportSummary = { eventCount: 0, newPhotos: 0, existingPhotos: 0, unselectedPhotos: 0, missingTitles: 0, emptySelected: 0 };
  for (const draft of drafts) {
    const edit = edits[draft.draftId];
    if (edit?.selected && isSelectable(draft)) {
      summary.eventCount += 1;
      summary.newPhotos += draft.newCount;
      summary.existingPhotos += draft.existingCount;
      if (!edit.title.trim()) summary.missingTitles += 1;
      if (draft.photoCount === 0) summary.emptySelected += 1;
    } else if (isSelectable(draft)) {
      summary.unselectedPhotos += draft.newCount;
    }
  }
  return summary;
}

export function buildCommit(
  preview: ImportPreview,
  mode: SourceMode,
  edits: DraftEdits,
  options: { requestId: Id; analyzeFaces: boolean; watch: boolean },
): CommitImportArgs {
  const drafts = draftsForMode(preview, mode)
    .filter((draft) => edits[draft.draftId]?.selected && isSelectable(draft))
    .map((draft) => {
      const edit = edits[draft.draftId] as DraftEdit;
      return { draftId: draft.draftId, title: edit.title.trim(), eventDate: edit.eventDate };
    });
  return { previewId: preview.previewId, requestId: options.requestId, mode, drafts, analyzeFaces: options.analyzeFaces, watch: options.watch };
}
