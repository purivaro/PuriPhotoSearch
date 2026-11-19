import { describe, expect, it } from 'vitest';
import type { PersonSummary } from '@/lib/ipc/types';
import { sortPeople } from './sortPeople';

const person = (id: string, displayName: string, photos: number, pinned = false) =>
  ({ id, displayName, confirmedPhotoCount: photos, aiPhotoCount: 0, pinned }) as PersonSummary;

const people = [person('1', 'กาน', 247), person('2', 'นัด', 273, true), person('3', 'จี๊ด', 84), person('4', 'ดร.ดาว', 23, true)];
const names = (list: PersonSummary[]) => list.map((p) => p.displayName);

describe('sortPeople', () => {
  it('คนที่ปักหมุดขึ้นก่อน แล้วเรียงตามชื่อในแต่ละส่วน', () => {
    expect(names(sortPeople(people, 'name'))).toEqual(['ดร.ดาว', 'นัด', 'กาน', 'จี๊ด']);
  });
  it('เรียงตามจำนวนรูปก็ยังให้คนที่ปักหมุดขึ้นก่อน', () => {
    expect(names(sortPeople(people, 'photos'))).toEqual(['นัด', 'ดร.ดาว', 'กาน', 'จี๊ด']);
  });
  it('ไม่แก้ลำดับของรายการเดิม', () => {
    const before = names(people);
    sortPeople(people, 'name');
    expect(names(people)).toEqual(before);
  });
});
