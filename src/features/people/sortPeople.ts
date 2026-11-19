import type { PersonSummary } from '@/lib/ipc/types';

export type PeopleSort = 'name' | 'photos';

/** คนที่ปักหมุดขึ้นก่อนเสมอ แล้วจึงเรียงตามที่เลือก (ชื่อ หรือจำนวนรูป) ภายในแต่ละส่วน */
export function sortPeople(people: PersonSummary[], sort: PeopleSort): PersonSummary[] {
  const byName = (a: PersonSummary, b: PersonSummary) => a.displayName.localeCompare(b.displayName, 'th') || a.id.localeCompare(b.id);
  const byPhotos = (a: PersonSummary, b: PersonSummary) => b.confirmedPhotoCount + b.aiPhotoCount - (a.confirmedPhotoCount + a.aiPhotoCount) || byName(a, b);
  const within = sort === 'photos' ? byPhotos : byName;
  return [...people].sort((a, b) => Number(b.pinned) - Number(a.pinned) || within(a, b));
}
