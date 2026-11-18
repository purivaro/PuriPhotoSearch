import { describe, expect, it } from 'vitest';
import { formatBytes, formatCount, formatTakenAt, formatThaiDate, middleEllipsis, parseIsoDate } from './format';

describe('formatThaiDate', () => {
  it('แสดงปี พ.ศ. จากวันที่ ค.ศ.', () => {
    expect(formatThaiDate('2026-11-18')).toBe('18 พ.ย. 2569');
    expect(formatThaiDate('2025-01-05')).toBe('5 ม.ค. 2568');
  });
  it('คืน null เมื่อไม่มีวันที่หรือรูปแบบผิด', () => {
    expect(formatThaiDate(null)).toBeNull();
    expect(formatThaiDate('18/11/2026')).toBeNull();
    expect(formatThaiDate('2026-13-01')).toBeNull();
  });
  it('ไม่เลื่อนวันเพราะ timezone', () => {
    expect(parseIsoDate('2026-01-01')).toEqual({ y: 2026, m: 1, d: 1 });
  });
});

describe('formatTakenAt', () => {
  it('คงเวลาท้องถิ่นจาก EXIF', () => {
    expect(formatTakenAt('2026-11-18T09:05:59')).toBe('18 พ.ย. 2569 09:05');
  });
  it('รับค่าที่มีแต่วัน', () => {
    expect(formatTakenAt('2026-11-18')).toBe('18 พ.ย. 2569');
  });
});

describe('ตัวเลขและขนาด', () => {
  it('คั่นหลักพัน', () => {
    expect(formatCount(12375)).toBe('12,375');
  });
  it('เลือกหน่วยขนาดไฟล์', () => {
    expect(formatBytes(0)).toBe('0 KB');
    expect(formatBytes(1536)).toBe('2 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatBytes(3.25 * 1024 ** 3)).toBe('3.3 GB');
  });
  it('ตัดกลาง path ยาว', () => {
    const out = middleEllipsis('/Users/someone/Photos/2569/691118 งานอบรม AI/กล้อง A/IMG_0001.JPG', 30);
    expect(out.length).toBe(30);
    expect(out).toContain('…');
    expect(out.endsWith('IMG_0001.JPG')).toBe(true);
  });
});
