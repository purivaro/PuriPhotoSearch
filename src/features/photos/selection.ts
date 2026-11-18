import type { Id, SelectionSpec } from '@/lib/ipc/types';

/**
 * ชุดรูปที่เลือก มีสองขอบเขตที่ต้องแยกให้ผู้ใช้เห็นเสมอ (Blueprint S18, 11.5)
 * - explicit: เฉพาะรูปที่ติ๊กเอง
 * - snapshot: "ผลค้นหาทั้งหมด" ที่ Rust แช่แข็งไว้ ณ ตอนกด ลบออกรายรูปได้ด้วย excluded
 */
export type Selection =
  | { kind: 'explicit'; ids: ReadonlySet<Id>; anchor: Id | null }
  | { kind: 'snapshot'; token: string; total: number; excluded: ReadonlySet<Id>; anchor: Id | null };

export const EMPTY_SELECTION: Selection = { kind: 'explicit', ids: new Set(), anchor: null };

export function isSelected(selection: Selection, id: Id): boolean {
  return selection.kind === 'explicit' ? selection.ids.has(id) : !selection.excluded.has(id);
}

export function selectionCount(selection: Selection): number {
  return selection.kind === 'explicit' ? selection.ids.size : Math.max(0, selection.total - selection.excluded.size);
}

export function toggle(selection: Selection, id: Id): Selection {
  if (selection.kind === 'explicit') {
    const ids = new Set(selection.ids);
    if (ids.has(id)) ids.delete(id);
    else ids.add(id);
    return { kind: 'explicit', ids, anchor: id };
  }
  const excluded = new Set(selection.excluded);
  if (excluded.has(id)) excluded.delete(id);
  else excluded.add(id);
  return { ...selection, excluded, anchor: id };
}

/** Shift+คลิก: เลือกทุกใบตั้งแต่จุดยึดล่าสุดถึงใบที่คลิก ตามลำดับที่แสดงอยู่ */
export function selectRange(selection: Selection, orderedIds: readonly Id[], toId: Id): Selection {
  const end = orderedIds.indexOf(toId);
  if (end < 0) return selection;
  const anchorIndex = selection.anchor ? orderedIds.indexOf(selection.anchor) : -1;
  if (anchorIndex < 0) return isSelected(selection, toId) ? { ...selection, anchor: toId } : toggle(selection, toId);
  const range = orderedIds.slice(Math.min(anchorIndex, end), Math.max(anchorIndex, end) + 1);
  if (selection.kind === 'explicit') {
    const ids = new Set(selection.ids);
    for (const id of range) ids.add(id);
    return { kind: 'explicit', ids, anchor: selection.anchor };
  }
  const excluded = new Set(selection.excluded);
  for (const id of range) excluded.delete(id);
  return { ...selection, excluded };
}

/** ⌘A: เลือกทุกใบที่โหลดอยู่ใน grid (ไม่ใช่ผลค้นหาทั้งหมด) */
export function selectLoaded(selection: Selection, loadedIds: readonly Id[]): Selection {
  if (selection.kind === 'snapshot') {
    const excluded = new Set(selection.excluded);
    for (const id of loadedIds) excluded.delete(id);
    return { ...selection, excluded };
  }
  return { kind: 'explicit', ids: new Set([...selection.ids, ...loadedIds]), anchor: selection.anchor };
}

export function selectSnapshot(token: string, total: number): Selection {
  return { kind: 'snapshot', token, total, excluded: new Set(), anchor: null };
}

export function toSelectionSpec(selection: Selection): SelectionSpec {
  return selection.kind === 'explicit'
    ? { kind: 'explicit', photoIds: [...selection.ids] }
    : { kind: 'snapshot', selectionToken: selection.token, excludedPhotoIds: [...selection.excluded] };
}
