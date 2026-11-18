import type { Id, PersonMode, SearchParams } from '@/lib/ipc/types';
import type { PhotoQuery } from '@/lib/queries';

/** เงื่อนไขค้นหาของหน้า S06 เก็บใน URL เพื่อย้อนกลับ/แชร์ภายในแอปได้ */
export interface SearchFilters {
  text: string;
  personIds: Id[];
  personMode: PersonMode;
  eventIds: Id[];
  dateField: 'photo_taken' | 'event_date';
  from: string;
  to: string;
  includeUnknownDate: boolean;
  faceFilter: SearchParams['faceFilter'];
  sort: SearchParams['sort'];
  /** นับรูปที่ AI จัดให้ด้วย (ค่าเริ่มต้น); ปิด = เฉพาะรูปที่มีคนยืนยันแล้ว */
  includeAi: boolean;
}

export const DEFAULT_FILTERS: SearchFilters = {
  text: '',
  personIds: [],
  personMode: 'all',
  eventIds: [],
  dateField: 'photo_taken',
  from: '',
  to: '',
  includeUnknownDate: false,
  faceFilter: 'any',
  sort: 'taken_desc',
  includeAi: true,
};

const SORTS: SearchParams['sort'][] = ['taken_desc', 'taken_asc', 'added_desc', 'filename'];
const FACE_FILTERS: SearchParams['faceFilter'][] = ['any', 'has_faces', 'no_person'];
const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

function list(value: string | null): Id[] {
  if (!value) return [];
  // ID ซ้ำใน URL ต้องไม่ทำให้เงื่อนไข/จำนวนเพี้ยน
  return [...new Set(value.split(',').map((v) => v.trim()).filter(Boolean))];
}

export function parseFilters(search: URLSearchParams): SearchFilters {
  const sort = search.get('sort') as SearchParams['sort'] | null;
  const face = search.get('face') as SearchParams['faceFilter'] | null;
  const from = search.get('from') ?? '';
  const to = search.get('to') ?? '';
  return {
    text: search.get('q') ?? '',
    personIds: list(search.get('p')),
    personMode: search.get('pm') === 'any' ? 'any' : 'all',
    eventIds: list(search.get('e')),
    dateField: search.get('df') === 'event_date' ? 'event_date' : 'photo_taken',
    from: ISO_DATE.test(from) ? from : '',
    to: ISO_DATE.test(to) ? to : '',
    includeUnknownDate: search.get('unk') === '1',
    faceFilter: face && FACE_FILTERS.includes(face) ? face : 'any',
    sort: sort && SORTS.includes(sort) ? sort : 'taken_desc',
    includeAi: search.get('ai') !== '0',
  };
}

/** เขียนเฉพาะค่าที่ต่างจากค่าเริ่มต้น URL จึงสั้นและเทียบกันได้ */
export function filtersToSearch(filters: SearchFilters): URLSearchParams {
  const out = new URLSearchParams();
  const text = filters.text.trim();
  if (text) out.set('q', text);
  if (filters.personIds.length) out.set('p', filters.personIds.join(','));
  if (filters.personIds.length > 1 && filters.personMode === 'any') out.set('pm', 'any');
  if (filters.eventIds.length) out.set('e', filters.eventIds.join(','));
  if (hasDateFilter(filters)) {
    if (filters.dateField === 'event_date') out.set('df', 'event_date');
    if (filters.from) out.set('from', filters.from);
    if (filters.to) out.set('to', filters.to);
    if (filters.includeUnknownDate) out.set('unk', '1');
  }
  if (filters.faceFilter !== 'any') out.set('face', filters.faceFilter);
  if (filters.sort !== 'taken_desc') out.set('sort', filters.sort);
  if (!filters.includeAi) out.set('ai', '0');
  return out;
}

export function hasDateFilter(filters: SearchFilters): boolean {
  return Boolean(filters.from || filters.to);
}

/** มีเงื่อนไขที่ทำให้ผลแคบลงหรือไม่ (ไม่นับการเรียง) */
export function hasActiveFilters(filters: SearchFilters): boolean {
  return Boolean(filters.text.trim() || filters.personIds.length || filters.eventIds.length || hasDateFilter(filters) || filters.faceFilter !== 'any');
}

export function filtersToQuery(filters: SearchFilters, limit: number): PhotoQuery {
  const text = filters.text.trim();
  const query: PhotoQuery = {
    personIds: filters.personIds,
    personMode: filters.personMode,
    eventIds: filters.eventIds,
    faceFilter: filters.faceFilter,
    sort: filters.sort,
    limit,
    includeSuggested: filters.includeAi,
  };
  if (text) query.text = text;
  if (hasDateFilter(filters)) {
    query.date = { field: filters.dateField, includeUnknown: filters.includeUnknownDate };
    if (filters.from) query.date.from = filters.from;
    if (filters.to) query.date.to = filters.to;
  }
  return query;
}

/** กุญแจเทียบว่า "เงื่อนไข" เปลี่ยนหรือไม่ (การเรียงไม่ทำให้ชุดผลเปลี่ยน) */
export function filterIdentity(filters: SearchFilters): string {
  const search = filtersToSearch({ ...filters, sort: 'taken_desc' });
  search.sort();
  return search.toString();
}

/** ลิงก์ค้นรูปที่ A และ B อยู่ด้วยกัน: ยืนยันแล้วเท่านั้น (ตรงกับตัวเลข "ปรากฏร่วม"), ต้องมีทุกคน (Blueprint 11.7) */
export function coappearanceSearchPath(personA: Id, personB: Id, scope?: Pick<SearchFilters, 'eventIds'>): string {
  const search = filtersToSearch({ ...DEFAULT_FILTERS, personIds: [personA, personB], personMode: 'all', eventIds: scope?.eventIds ?? [], includeAi: false });
  return `/search?${search.toString()}`;
}

export function personSearchPath(personId: Id): string {
  return `/search?${filtersToSearch({ ...DEFAULT_FILTERS, personIds: [personId] }).toString()}`;
}

export function textSearchPath(text: string): string {
  return `/search?${filtersToSearch({ ...DEFAULT_FILTERS, text }).toString()}`;
}
