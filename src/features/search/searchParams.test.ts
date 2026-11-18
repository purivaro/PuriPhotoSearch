import { describe, expect, it } from 'vitest';
import { DEFAULT_FILTERS, coappearanceSearchPath, filterIdentity, filtersToQuery, filtersToSearch, hasActiveFilters, parseFilters } from './searchParams';

describe('เงื่อนไขค้นหา ↔ URL', () => {
  it('ค่าเริ่มต้นไม่เขียนอะไรลง URL', () => {
    expect(filtersToSearch(DEFAULT_FILTERS).toString()).toBe('');
    expect(parseFilters(new URLSearchParams(''))).toEqual(DEFAULT_FILTERS);
    expect(hasActiveFilters(DEFAULT_FILTERS)).toBe(false);
  });

  it('ไปกลับได้ค่าเดิม รวมข้อความไทยและเครื่องหมาย', () => {
    const filters = {
      ...DEFAULT_FILTERS,
      text: 'พระมหา "อนวัช" 100%',
      personIds: ['p1', 'p2'],
      personMode: 'any' as const,
      eventIds: ['e9'],
      dateField: 'event_date' as const,
      from: '2026-11-01',
      to: '2026-11-30',
      includeUnknownDate: true,
      faceFilter: 'no_person' as const,
      sort: 'filename' as const,
    };
    const round = parseFilters(new URLSearchParams(filtersToSearch(filters).toString()));
    expect(round).toEqual(filters);
  });

  it('ตัด ID ซ้ำ และไม่รับค่าที่ไม่รู้จัก', () => {
    const parsed = parseFilters(new URLSearchParams('p=a,b,a,,b&pm=weird&sort=drop&face=x&from=18/11/2026'));
    expect(parsed.personIds).toEqual(['a', 'b']);
    expect(parsed.personMode).toBe('all');
    expect(parsed.sort).toBe('taken_desc');
    expect(parsed.faceFilter).toBe('any');
    expect(parsed.from).toBe('');
  });

  it('โหมด any มีความหมายเมื่อเลือกเกินหนึ่งคนเท่านั้น', () => {
    const one = filtersToSearch({ ...DEFAULT_FILTERS, personIds: ['a'], personMode: 'any' });
    expect(one.has('pm')).toBe(false);
  });
});

describe('เงื่อนไข → คำขอไป Rust', () => {
  it('ไม่ส่ง date เมื่อไม่ได้กำหนดช่วง และตัดช่องว่างของข้อความ', () => {
    const query = filtersToQuery({ ...DEFAULT_FILTERS, text: '  ภูริ ', includeUnknownDate: true }, 100);
    expect(query.text).toBe('ภูริ');
    expect(query.date).toBeUndefined();
    expect(query.limit).toBe(100);
  });

  it('ส่งช่วงวันที่ตาม field ที่เลือก', () => {
    const query = filtersToQuery({ ...DEFAULT_FILTERS, dateField: 'event_date', from: '2026-11-18' }, 50);
    expect(query.date).toEqual({ field: 'event_date', includeUnknown: false, from: '2026-11-18' });
  });

  it('การเรียงไม่ถือว่าเงื่อนไขเปลี่ยน', () => {
    const a = { ...DEFAULT_FILTERS, personIds: ['x'] };
    expect(filterIdentity(a)).toBe(filterIdentity({ ...a, sort: 'filename' }));
    expect(filterIdentity(a)).not.toBe(filterIdentity({ ...a, personIds: ['y'] }));
  });

  it('ลิงก์ปรากฏร่วมในภาพเป็น AND ของสองคน', () => {
    const path = coappearanceSearchPath('a', 'b');
    const parsed = parseFilters(new URLSearchParams(path.split('?')[1]));
    expect(parsed.personIds).toEqual(['a', 'b']);
    expect(parsed.personMode).toBe('all');
  });
});
