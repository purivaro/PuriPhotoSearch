import type { Id, ReviewFace, ReviewOverview } from '@/lib/ipc/types';

export type ReviewTab = 'suggested' | 'unknown';

/** หนึ่งรายการในคิวซ้าย: กลุ่มที่ AI เสนอชื่อ (id = personId) หรือกลุ่มที่ยังไม่ทราบชื่อ (id = clusterId) */
export interface QueueGroup {
  id: Id;
  kind: 'person' | 'cluster';
  faceCount: number;
  sampleFaceIds: Id[];
}

export function queueGroups(overview: ReviewOverview, tab: ReviewTab): QueueGroup[] {
  return tab === 'suggested'
    ? overview.suggested.map((g) => ({ id: g.person.id, kind: 'person' as const, faceCount: g.faceCount, sampleFaceIds: g.sampleFaceIds }))
    : overview.clusters.map((c) => ({ id: c.clusterId, kind: 'cluster' as const, faceCount: c.faceCount, sampleFaceIds: c.sampleFaceIds }));
}

/** แท็บเริ่มต้น: เปิดแท็บที่มีงานให้ทำ (ข้อเสนอของ AI ก่อน เพราะตรวจเร็วกว่า) */
export function defaultTab(overview: ReviewOverview): ReviewTab {
  return overview.suggested.length === 0 && overview.clusters.length > 0 ? 'unknown' : 'suggested';
}

/**
 * เลือกกลุ่มหลังรายการคิวเปลี่ยน (เช่นเพิ่งยืนยันกลุ่มหนึ่งไป):
 * กลุ่มเดิมยังอยู่ → อยู่ที่เดิม; หายไปแล้ว → กลุ่มที่เลื่อนขึ้นมาแทนตำแหน่งเดิม; ไม่มีกลุ่มเหลือ → null
 */
export function resolveGroup(groups: { id: Id }[], previous: { id: Id; index: number } | null): Id | null {
  if (groups.length === 0) return null;
  if (previous) {
    if (groups.some((g) => g.id === previous.id)) return previous.id;
    return groups[Math.min(Math.max(previous.index, 0), groups.length - 1)]?.id ?? null;
  }
  return groups[0]?.id ?? null;
}

/** ค่าเริ่มต้นของการเลือก: ทุกใบที่โหลดมาแสดง (ผู้ใช้เห็นครบและเอาใบที่ผิดออกก่อนยืนยัน) */
export function selectAllFaces(faces: Pick<ReviewFace, 'faceId'>[]): Set<Id> {
  return new Set(faces.map((f) => f.faceId));
}

export function toggleFace(selection: ReadonlySet<Id>, faceId: Id): Set<Id> {
  const next = new Set(selection);
  if (next.has(faceId)) next.delete(faceId);
  else next.add(faceId);
  return next;
}

/** ใบหน้าที่เลือก ตามลำดับที่แสดง และเฉพาะใบที่อยู่ในรายการที่โหลดจริง (ไม่ยืนยันใบที่ผู้ใช้ไม่เห็น) */
export function selectedFaceIds(faces: Pick<ReviewFace, 'faceId'>[], selection: ReadonlySet<Id>): Id[] {
  return faces.filter((f) => selection.has(f.faceId)).map((f) => f.faceId);
}

export function selectionCheckState(faces: Pick<ReviewFace, 'faceId'>[], selection: ReadonlySet<Id>): boolean | 'indeterminate' {
  const count = selectedFaceIds(faces, selection).length;
  if (count === 0) return false;
  return count === faces.length ? true : 'indeterminate';
}

/** รูปของใบหน้าในกลุ่ม ไม่ซ้ำ ตามลำดับที่แสดง — ใช้เป็นลำดับ ←/→ ของตัวดูภาพ */
export function photoIdsOf(faces: Pick<ReviewFace, 'photoId'>[]): Id[] {
  return [...new Set(faces.map((f) => f.photoId))];
}

/** ป้ายระดับของข้อเสนอ AI: เป็นคำ ไม่ใช่เปอร์เซ็นต์ (ยังไม่มีการ calibrate คะแนน) */
export function levelLabel(level: 'close' | 'check'): string {
  return level === 'close' ? 'ใกล้เคียงมาก' : 'ควรตรวจสอบ';
}
